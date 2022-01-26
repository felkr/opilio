#[macro_use]
extern crate html5ever;
extern crate markup5ever_rcdom as rcdom;
extern crate sdl2;

use crate::colorscheme::DefaultColorSchemes;
use crate::renderer::*;

use std::cell::RefCell;
use std::collections::HashMap;

use std::io::{self, BufRead, BufReader};

use std::rc::Rc;

use std::str::FromStr;
use std::{env, fs};

use html5ever::parse_document;
use html5ever::tendril::TendrilSink;

use rcdom::RcDom;
use sdl2::event::{Event, WindowEvent};

use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use std::default::Default;

use clap::Parser;
use std::string::String;

mod colorscheme;
mod renderer;

static SCREEN_WIDTH: u32 = 800;
static SCREEN_HEIGHT: u32 = 600;
static SCROLL_SPEED: i32 = 12;
static DRAW_HITRECTS: bool = false;
// static BG_COLOR: Color = Color::WHITE;
// static FG_COLOR: Color = Color::BLACK;

// handle the annoying Rect i32
macro_rules! rect(
    ($x:expr, $y:expr, $w:expr, $h:expr) => (
        Rect::new($x as i32, $y as i32, $w as u32, $h as u32)
    )
);

// Scale fonts to a reasonable size when they're too big (though they might look less smooth)
fn get_centered_rect(rect_width: u32, rect_height: u32, cons_width: u32, cons_height: u32) -> Rect {
    let wr = rect_width as f32 / cons_width as f32;
    let hr = rect_height as f32 / cons_height as f32;

    let (w, h) = if wr > 1f32 || hr > 1f32 {
        if wr > hr {
            println!("Scaling down! The text will look worse!");
            let h = (rect_height as f32 / wr) as i32;
            (cons_width as i32, h)
        } else {
            println!("Scaling down! The text will look worse!");
            let w = (rect_width as f32 / hr) as i32;
            (w, cons_height as i32)
        }
    } else {
        (rect_width as i32, rect_height as i32)
    };

    let cx = (SCREEN_WIDTH as i32 - w) / 2;
    let cy = (SCREEN_HEIGHT as i32 - h) / 2;
    rect!(cx, cy, w, h)
}
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, arg_enum, default_value = "standard")]
    color_theme: DefaultColorSchemes,

    file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsys = sdl_context.video()?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;
    let args = Args::parse();

    let window = video_subsys
        .window("SDL2_TTF Example", SCREEN_WIDTH, SCREEN_HEIGHT)
        .position_centered()
        .resizable()
        .vulkan()
        .allow_highdpi()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas.window_mut().set_minimum_size(400, 400).unwrap();
    let texture_creator = canvas.texture_creator();

    let mut input: Box<dyn BufRead> = match args.file {
        None => Box::new(BufReader::new(io::stdin())),
        Some(filename) => Box::new(BufReader::new(
            fs::File::open(filename).expect("Couldn't open file"),
        )),
    };

    /* let mut strstr = String::new();
     * input.read_to_string(&mut strstr).unwrap();
     * let tldom = tl::parse(strstr.as_str(), tl::ParserOptions::default()).unwrap();
     * let parser = tldom.parser();
     * tldom.nodes().iter().for_each(|node| {
     *     if let Some(tag) = node.as_tag() {
     *         if tag.name().as_bytes() == "p".as_bytes() {
     *             println!("{:?}", tag.children().nth(0).unwrap().get_inner());
     *             println!("{:?}: {:?}", tag.name(), node.inner_html(parser));
     *         }
     *     }
     * }); */

    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut input)
        .unwrap();
    // print_dom(0, &dom.document);

    let sf = canvas.output_size().unwrap().0 / canvas.window().size().0;
    macro_rules! load_font {
        () => {
            ttf_context
                .load_font("/usr/share/fonts/TTF/Times.TTF", 50 * sf as u16)
                .unwrap_or_else(|_| {
                    ttf_context
                        .load_font("assets/trim.ttf", 50 * sf as u16)
                        .expect("Could neither load system font nor fallback!")
                })
        };
    }
    let mut rc = RendererContext {
        canvas: Rc::new(RefCell::new(canvas)),
        font: Rc::new(RefCell::new(load_font!())),
        texture_creator: Rc::new(texture_creator),
        scaling_factor: sf,
        images: HashMap::new(),
        viewport: (0, 0),
        hit_map: Vec::new(),
        color_scheme: args.color_theme.value(),
        indices: (12, 12),
    };

    rc.canvas
        .borrow_mut()
        .set_draw_color(rc.color_scheme.background);
    rc.canvas.borrow_mut().clear();

    rc.font.borrow_mut().set_style(sdl2::ttf::FontStyle::NORMAL);

    rc.canvas.borrow_mut().present();

    'mainloop: loop {
        for event in sdl_context.event_pump()?.poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::Quit { .. } => break 'mainloop,
                Event::MouseWheel { x, y, .. } => {
                    rc.viewport.0 += x * SCROLL_SPEED;
                    rc.viewport.1 += y * SCROLL_SPEED;
                    if rc.viewport.1 > 0 {
                        rc.viewport.1 = 0;
                    }
                    rc.canvas.borrow_mut().clear();
                    rc.hit_map.clear();
                    render(0, &dom.document, "", &mut rc).await;
                    rc.canvas.borrow_mut().present();
                    rc.indices.1 = 0;
                }
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    for hit_rect in &rc.hit_map {
                        // println!("{:?}", hit_rect);
                        if hit_rect.0 <= x * rc.scaling_factor as i32
                            && hit_rect.0 + hit_rect.2 as i32 >= x * rc.scaling_factor as i32
                            && hit_rect.1 <= y * rc.scaling_factor as i32
                            && hit_rect.1 + hit_rect.3 as i32 >= y * rc.scaling_factor as i32
                        {
                            hit_rect.4();
                        }
                    }
                }
                Event::Window { win_event, .. } => match win_event {
                    WindowEvent::Resized(w, h) => {
                        rc.canvas
                            .borrow_mut()
                            .window_mut()
                            .set_size(w as u32, h as u32)
                            .unwrap();
                        rc.canvas
                            .borrow_mut()
                            .set_draw_color(rc.color_scheme.background);
                        rc.canvas.borrow_mut().clear();
                        rc.hit_map.clear();
                        render(0, &dom.document, "", &mut rc).await;
                        if DRAW_HITRECTS {
                            for hit_rect in &rc.hit_map {
                                rc.canvas.borrow_mut().set_draw_color(Color::RED);
                                rc.canvas.borrow_mut().draw_rect(rect!(
                                    hit_rect.0, hit_rect.1, hit_rect.2, hit_rect.3
                                ))?;
                            }
                        }
                        rc.canvas.borrow_mut().present();
                        rc.indices.1 = 0;
                    }
                    _ => {
                        let c = rc.canvas.borrow_mut();
                        if c.output_size().unwrap().0 / c.window().size().0 != rc.scaling_factor {
                            rc.scaling_factor = c.output_size().unwrap().0 / c.window().size().0;
                            rc.font = Rc::new(RefCell::new(load_font!()));
                        }
                    }
                },
                _ => {}
            }
        }
        sdl_context.timer()?.delay(1000 / 165);
    }

    Ok(())
}
