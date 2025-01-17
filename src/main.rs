use clap::Parser;
use enigo::{Button, Coordinate, Direction::{Click, Press, Release}, Enigo, Key, Keyboard, Mouse, Settings};
use log::{info, error};
use rand::prelude::*;
use simplelog::*;
use std::io::Write;
use std::{io, thread, time};

const ENDOFAUTO: &str = r#"

############################################
Automated mouse typing and clicking complete
Thank you for using this program!
############################################
"#;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// click or move or type
    action: String,

    /// Number of seconds between moving or clicking
    #[arg(short, long, default_value_t = 60)]
    interval: u64,

    /// Number of iterations to run program
    #[arg(short, long, default_value_t = 60)]
    count: u16,
}

fn main() {
    let args = Args::parse();

    if args.action == "move" {
        move_mouse(args.interval, args.count)
    } else if args.action == "click" {
        click_mouse(args.interval, args.count)
    } else if args.action == "type" {
        type_stuff(args.interval, args.count)
    }
}

fn move_mouse(interval: u64, count: u16) {
    if let Err(e) = TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ) {
        println!("{:?}", e)
    };
    info!(
        "Moving cursor for: {} hours",
        (interval as f32 * count as f32) / 3600.
    );

    let mut rng = rand::thread_rng();
    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    let mut n = 0;
    while n < count {
        let x: i32 = rng.gen_range(0..500);
        let y: i32 = rng.gen_range(0..500);
        info!("x: {} \t y: {}", x, y);
        if let Err(e) = io::stdout().flush() {
            println!("{:?}", e)
        }
        enigo.move_mouse(x, y, Coordinate::Rel).expect("TODO: panic message");
        thread::sleep(time::Duration::from_secs(interval));
        n += 1;
    }
    println!("{}", ENDOFAUTO);
}

fn click_mouse(interval: u64, count: u16) {
    if let Err(e) = TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ) {
        println!("{:?}", e)
    };
    info!(
        "Clicking left mouse button for: {} hours",
        (interval as f32 * count as f32) / 3600.
    );

    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    let mut n = 0;
    while n < count {
        // let cursor_location: (i32, i32) = enigo.mouse_location();
        // info!("Clicked at x: {} \t y: {}", cursor_location.0, cursor_location.1);
        info!("Clicked!");
        if let Err(e) = io::stdout().flush() {
            println!("{:?}", e)
        }
        enigo.button(Button::Left, Press).expect("TODO: panic message");
        enigo.button(Button::Left, Release).expect("TODO: panic message");
        thread::sleep(time::Duration::from_secs(interval));
        n += 1;
    }
    println!("{}", ENDOFAUTO);
}

fn type_stuff(interval: u64, count: u16){
    if let Err(e) = TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ) {
        println!("{:?}", e)
    };
    info!(
        "Typing for: {} hours",
        (interval as f32 * count as f32) / 3600.
    );

    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    let mut n = 0;
    let mut rng = rand::thread_rng();
    while n < count {
        // let cursor_location: (i32, i32) = enigo.mouse_location();
        // info!("Clicked at x: {} \t y: {}", cursor_location.0, cursor_location.1);
        if let Err(e) = io::stdout().flush() {
            println!("{:?}", e)
        }
        let random_char = rng.gen_range(b'a'..=b'z') as char;
        match enigo.key(Key::Unicode(random_char), Click) {
            Ok(_) => info!("Typed!"),
            Err(e) => error!("Error: {}", e)
        }
        thread::sleep(time::Duration::from_secs(interval));
        n += 1;
    }
    println!("{}", ENDOFAUTO);
}
