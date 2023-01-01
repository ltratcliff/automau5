use clap::Parser;
use enigo::*;
use log::info;
use rand::prelude::*;
use simplelog::*;
use std::io::Write;
use std::{io, thread, time};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// click or move
    action: String,

    /// Number of seconds between moving
    #[arg(short, long, default_value_t = 5)]
    interval: u64,

    /// Number of moves to run program
    #[arg(short, long, default_value_t = 60)]
    duration: u16,
}

fn main() {
    let args = Args::parse();
    if args.action == "move" {
        move_mouse(args.interval, args.duration)
    } else if args.action == "click" {
        // println!("Not implemented yet");
        // process::exit(1);
        click_mouse(args.interval, args.duration)
    }

    //
}

fn move_mouse(interval: u64, duration: u16) {
    match TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ) {
        Err(e) => println!("{:?}", e),
        _ => (),
    };

    let mut rng = rand::thread_rng();
    let mut enigo = Enigo::new();
    let mut n = 0;
    while n < duration {
        let x: i32 = rng.gen_range(0..500);
        let y: i32 = rng.gen_range(0..500);
        info!("x: {} \t y: {}", x, y);
        match io::stdout().flush() {
            Err(e) => println!("{:?}", e),
            _ => (),
        }
        enigo.mouse_move_to(x, y);
        thread::sleep(time::Duration::from_secs(interval));
        n += 1;
    }
}

fn click_mouse(interval: u64, duration: u16) {
    match TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ) {
        Err(e) => println!("{:?}", e),
        _ => (),
    };

    let mut enigo = Enigo::new();
    let mut n = 0;
    while n < duration {
        // let cursor_location: (i32, i32) = enigo.mouse_location();
        // info!("Clicked at x: {} \t y: {}", cursor_location.0, cursor_location.1);
        info!("Clicked!");
        match io::stdout().flush() {
            Err(e) => println!("{:?}", e),
            _ => (),
        }
        enigo.mouse_click(MouseButton::Left);
        thread::sleep(time::Duration::from_secs(interval));
        n += 1;
    }
}
