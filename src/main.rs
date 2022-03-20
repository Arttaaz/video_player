use std::path::PathBuf;
use std::io::prelude::*;

extern crate ffmpeg_next as ffmpeg;
use ffmpeg::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Args {
    #[structopt(short, long, parse(from_os_str))]
    input: PathBuf,
}

struct SquareWave {
    phase_inc: f32,
    phase: f32,
    volume: f32,
}

impl sdl2::audio::AudioCallback for SquareWave {
    type Channel = f32;

    fn callback(&mut self, out: &mut [f32]) {
        // Generate a square wave
        for x in out.iter_mut() {
            *x = if self.phase <= 0.5 {
                self.volume
            } else {
                -self.volume
            };
            self.phase_inc += self.phase_inc.atan2(self.phase.cosh()); //pretty nice
            self.phase = (self.phase + self.phase_inc) % 1.0;
        }
    }
}

fn main() {
    let args = Args::from_args();

    let sdl = sdl2::init().unwrap();

    match ffmpeg::init() {
        Ok(_) => (),
        Err(e) => eprintln!("Error: {}", e),
    }

    let mut input = match format::input(&args.input) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open input file: {}", e);
            return
        }
    };
    
    let input_stream = match input.streams().best(media::Type::Video).ok_or(Error::StreamNotFound) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return
        }
    };
    let stream_index = input_stream.index();
    let stream = input_stream.codec().decoder().clone();//ffmpeg::codec::context::Context::from_parameters(input_stream.parameters()).unwrap();
    let mut decoder = stream.decoder().video().unwrap();
    let frame_rate_fdp = decoder.frame_rate().unwrap();

    let input_audio_stream = match input.streams().best(media::Type::Audio).ok_or(Error::StreamNotFound) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return
        }
    };
    let audio_stream_index = input_audio_stream.index();
    let audio_stream = ffmpeg::codec::context::Context::from_parameters(input_audio_stream.parameters()).unwrap();
    let mut audio_decoder = audio_stream.decoder().audio().unwrap();
    audio_decoder.set_parameters(input_audio_stream.parameters()).unwrap();

    let mut scalar = software::scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            format::Pixel::YUV420P,
            decoder.width(),
            decoder.height(),
            software::scaling::flag::Flags::LANCZOS
        ).unwrap();

    let video_subsystem = sdl.video().unwrap();
    let window = video_subsystem
        .window(
            "Twitch player but better",
            decoder.width(),
            decoder.height())
        .position_centered()
        .opengl()
        .build().unwrap();

    let mut canvas = window.into_canvas().build().unwrap();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture_streaming(sdl2::pixels::PixelFormatEnum::YV12, decoder.width(), decoder.height()).unwrap();

    let audio_subsystem = sdl.audio().unwrap();
    let desired_spec = sdl2::audio::AudioSpecDesired {
        freq: Some(audio_decoder.rate() as i32),
        channels: Some(audio_decoder.channels() as u8),
        samples: None,
    };
    let audio_device = audio_subsystem.open_playback(None, &desired_spec, |spec| {
        let mut buff: [u8; 200000] = [69; 200000];
        buff.fill(50);
        SquareWave {
            phase_inc: 440.0 / spec.freq as f32,
            phase: 0.0,
            volume: 0.05,
        }
    }).unwrap();
                audio_device.resume();

    let mut event_pump = sdl.event_pump().unwrap();

    'yolo: for (s, p) in input.packets() {
        if s.index() == stream_index {
            decoder.send_packet(&p).unwrap();
            let mut decoded = util::frame::Video::empty();
            while decoder.receive_frame(&mut decoded).is_ok() {
                let mut rgb_frame = util::frame::Video::empty();
                scalar.run(&decoded, &mut rgb_frame).unwrap();
                process_frame(&mut scalar, &mut decoded, &mut texture);

                canvas.clear();
                canvas.copy(&texture, None, Some(sdl2::rect::Rect::new(0, 0, decoder.width(), decoder.height()))).unwrap();
                canvas.present();

            }
            std::thread::sleep(std::time::Duration::from_nanos(1_000_000_000/(frame_rate_fdp.0/frame_rate_fdp.1) as u64));
        } 
        //else if s.index() == audio_stream_index {
        //    p.rescale_ts(audio_decoder.time_base(), audio_decoder.time_base());
        //    audio_decoder.send_packet(&p).unwrap();
        //    let mut decoded = frame::Audio::empty();
        //    while audio_decoder.receive_frame(&mut decoded).is_ok() {
        //        let timestamp = decoded.timestamp();
        //        decoded.set_pts(timestamp);
        //        //audio_device.queue_audio(decoded.data(0)).unwrap();
        //        audio_device.resume();
        //        
        //    }
        //}
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => break 'yolo,
                _ => {},
            }
        }
    }
    
    decoder.send_eof().unwrap();
    let mut decoded = util::frame::Video::empty();
    while decoder.receive_frame(&mut decoded).is_ok() {
        let mut rgb_frame = util::frame::Video::empty();
        scalar.run(&decoded, &mut rgb_frame).unwrap();
        process_frame(&mut scalar, &mut decoded, &mut texture);
    }
    audio_device.pause();
}

fn process_frame(scalar: &mut software::scaling::Context, decoded: &mut frame::Video, texture: &mut sdl2::render::Texture) {
    let mut rgb_frame = util::frame::Video::empty();
    scalar.run(&decoded, &mut rgb_frame).unwrap();
    texture.with_lock(None, |mut buffer: &mut [u8], _pitch: usize| {
        buffer.write(decoded.data(0)).unwrap();
        buffer.write(decoded.data(1)).unwrap();
        buffer.write(decoded.data(2)).unwrap();
    }).unwrap();

}
