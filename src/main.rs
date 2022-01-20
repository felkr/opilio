#[macro_use]
extern crate html5ever;
extern crate markup5ever_rcdom as rcdom;
extern crate sdl2;

use std::borrow::{self, Borrow};
use std::cell::RefCell;
use std::collections::HashMap;

use std::io::{self};
use std::ops::RangeBounds;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use async_recursion::async_recursion;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;

use hyper::Uri;
use rcdom::RcDom;
use sdl2::event::{Event, WindowEvent};
use sdl2::image::{LoadSurface, LoadTexture};
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::surface::Surface;
use sdl2::ttf::FontStyle;
use sdl2::video::WindowContext;

use std::default::Default;

use std::string::String;

use rcdom::{Handle, NodeData};

static SCREEN_WIDTH: u32 = 800;
static SCREEN_HEIGHT: u32 = 600;
static SCROLL_SPEED: i32 = 12;
static BG_COLOR: Color = Color::WHITE;
static FG_COLOR: Color = Color::BLACK;

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
#[derive(Clone)]
struct RendererContext<'a> {
    canvas: Rc<RefCell<WindowCanvas>>,
    font: Rc<RefCell<sdl2::ttf::Font<'a, 'a>>>,
    texture_creator: Rc<TextureCreator<WindowContext>>,
    scaling_factor: u32,
    images: HashMap<String, Vec<u8>>,
    viewport: (i32, i32),
    hit_map: Vec<(i32, i32, u32, u32, fn())>,
}
#[async_recursion(?Send)]
async fn render<'a>(
    indent: usize,
    handle: &Handle,
    tag_name: &str,
    text_index: &mut u32,
    context: &'a mut RendererContext,
) {
    let node = handle;
    let mut next_tag_name = "";
    let invisible_tags = [
        "style", "script", "head", "title", "meta", "link", "img", "br",
    ];
    match node.data {
        NodeData::Text { ref contents } => {
            if tag_name == "title" {
                context
                    .canvas
                    .borrow_mut()
                    .window_mut()
                    .set_title(&contents.borrow())
                    .unwrap();
            }

            if &contents.borrow().trim().len() != &0 && !invisible_tags.contains(&tag_name) {
                let mut text_color = FG_COLOR;
                let (mut width, mut height) = context
                    .font
                    .borrow_mut()
                    .size_of(&contents.borrow())
                    .unwrap();
                let mut font_size = 12 * context.scaling_factor;
                if tag_name == "a" {
                    context.font.borrow_mut().set_style(FontStyle::UNDERLINE);
                    text_color = Color::RGB(0, 0, 238);
                }
                if tag_name.starts_with("h") {
                    let font_sizes = [32, 24, 19, 16, 13, 11];
                    font_size = font_sizes[tag_name
                        .chars()
                        .nth(1)
                        .unwrap_or('1')
                        .to_string()
                        .parse::<usize>()
                        .unwrap()
                        - 1]
                        * context.scaling_factor;

                    context.font.borrow_mut().set_style(FontStyle::BOLD);
                }
                let ratio = font_size as f32 / height as f32;
                width = (width as f32 * ratio).ceil() as u32;
                height = font_size;
                let surface = context
                    .font
                    .borrow_mut()
                    .render(&contents.borrow())
                    .blended(text_color)
                    .map_err(|e| e.to_string())
                    .unwrap();

                let texture = context
                    .texture_creator
                    .create_texture_from_surface(&surface)
                    .map_err(|e| e.to_string())
                    .unwrap();
                context.hit_map.push((
                    (0 + context.viewport.0),
                    *text_index as i32 + context.viewport.1,
                    width,
                    height,
                    || println!("Hello, I'm text"),
                ));
                context
                    .canvas
                    .borrow_mut()
                    .copy(
                        &texture,
                        None,
                        rect!(
                            0 + context.viewport.0,
                            *text_index as i32 + context.viewport.1,
                            width,
                            height
                        ),
                    )
                    .unwrap();
                context.font.borrow_mut().set_style(FontStyle::NORMAL);
                *text_index += height as u32;
            }
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            next_tag_name = &name.local;
            if &name.local == "img" {
                let img_path = attrs
                    .borrow()
                    .iter()
                    .find(|a| &a.name.local == "src")
                    .unwrap()
                    .value
                    .to_string();
                let mut texture: Texture = context
                    .texture_creator
                    .create_texture_static(PixelFormatEnum::RGBA8888, 1, 1)
                    .unwrap();
                let (mut width, mut height) = (0, 0);
                if img_path.starts_with("http://") || img_path.starts_with("https://") {
                    if !context.images.contains_key(&img_path) {
                        println!("Requesting {}...", img_path);
                        let client = hyper::Client::new();
                        let res = client
                            .get(Uri::from_str(img_path.as_str()).unwrap())
                            .await
                            .unwrap();
                        let bytes = hyper::body::to_bytes(res).await.unwrap().to_vec();
                        context.images.insert(img_path.clone(), bytes);
                    }

                    texture = context
                        .texture_creator
                        .load_texture_bytes(context.images.get(&img_path).unwrap())
                        .unwrap();
                    let query = texture.query();
                    width = query.width * context.scaling_factor;
                    height = query.height * context.scaling_factor;
                }
                if let Ok(surface) = Surface::from_file(Path::new(&img_path)) {
                    width = surface.width() * context.scaling_factor;
                    height = surface.height() * context.scaling_factor;
                    texture = context
                        .texture_creator
                        .create_texture_from_surface(&surface)
                        .unwrap();
                } else {
                    println!("Couldn't load image: {}", img_path);
                }
                context.hit_map.push((
                    0 + context.viewport.0,
                    *text_index as i32 + context.viewport.1,
                    width,
                    height,
                    || println!("Hello, I'm an image"),
                ));
                context
                    .canvas
                    .borrow_mut()
                    .copy(
                        &texture,
                        None,
                        rect!(
                            0 + context.viewport.0,
                            *text_index as i32 + context.viewport.1,
                            width,
                            height
                        ),
                    )
                    .unwrap();
                *text_index += height;
            }
        }
        NodeData::ProcessingInstruction { .. } => unreachable!(),
        _ => {}
    }
    for child in node.children.borrow().iter() {
        render(indent + 1, child, next_tag_name, text_index, context).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsys = sdl_context.video()?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;

    let window = video_subsys
        .window("SDL2_TTF Example", SCREEN_WIDTH, SCREEN_HEIGHT)
        .position_centered()
        .resizable()
        .vulkan()
        .allow_highdpi()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let texture_creator = canvas.texture_creator();

    canvas.set_draw_color(BG_COLOR);
    canvas.clear();

    let stdin = io::stdin();
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut stdin.lock())
        .unwrap();

    let sf = canvas.output_size().unwrap().0 / canvas.window().size().0;
    macro_rules! load_font {
        () => {
            ttf_context
                .load_font("/usr/share/fonts/TTF/Times.TTF", 12 * sf as u16)
                .unwrap_or_else(|_| {
                    ttf_context
                        .load_font("assets/trim.ttf", 12 * sf as u16)
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
    };
    let mut text_index: u32 = 0;
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
                    render(0, &dom.document, "", &mut text_index, &mut rc).await;
                    rc.canvas.borrow_mut().present();
                    text_index = 0;
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
                        rc.canvas.borrow_mut().set_draw_color(BG_COLOR);
                        rc.canvas.borrow_mut().clear();
                        rc.hit_map.clear();
                        render(0, &dom.document, "", &mut text_index, &mut rc).await;
                        for hit_rect in &rc.hit_map {
                            rc.canvas.borrow_mut().set_draw_color(Color::RED);
                            rc.canvas
                                .borrow_mut()
                                .draw_rect(rect!(hit_rect.0, hit_rect.1, hit_rect.2, hit_rect.3));
                        }
                        rc.canvas.borrow_mut().present();
                        text_index = 0;
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
    }

    Ok(())
}
