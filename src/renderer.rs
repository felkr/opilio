use crate::FG_COLOR;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;

use std::ops::RangeBounds;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use async_recursion::async_recursion;

use html5ever::tendril::TendrilSink;

use hyper::Uri;

use sdl2::image::{LoadSurface, LoadTexture};

use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::surface::Surface;
use sdl2::ttf::FontStyle;
use sdl2::video::WindowContext;

use std::string::String;

use rcdom::{Handle, NodeData};
// handle the annoying Rect i32
macro_rules! rect(
    ($x:expr, $y:expr, $w:expr, $h:expr) => (
        Rect::new($x as i32, $y as i32, $w as u32, $h as u32)
    )
);

#[derive(Clone)]
pub struct RendererContext<'a> {
    pub canvas: Rc<RefCell<WindowCanvas>>,
    pub font: Rc<RefCell<sdl2::ttf::Font<'a, 'a>>>,
    pub texture_creator: Rc<TextureCreator<WindowContext>>,
    pub scaling_factor: u32,
    pub images: HashMap<String, Vec<u8>>,
    pub viewport: (i32, i32),
    pub hit_map: Vec<(i32, i32, u32, u32, fn())>,
}
#[async_recursion(?Send)]
pub async fn render<'a>(
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
                let mut overflow = String::new();
                loop {
                    let mut text_color = FG_COLOR;
                    let mut text = contents.borrow().to_string();
                    if overflow.len() > 0 {
                        text = overflow;
                        overflow = String::new();
                    }
                    let (mut width, mut height) = context.font.borrow_mut().size_of(&text).unwrap();
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
                    let mut c1 = text.len() as u32 * font_size;
                    let c2 =
                        context.canvas.borrow_mut().window().size().0 * 2 * context.scaling_factor;
                    while c1 > c2 {
                        overflow.insert(0, text.pop().unwrap());
                        width = context.font.borrow_mut().size_of(&text).unwrap().0;
                        width = (width as f32 * ratio).ceil() as u32;
                        c1 = text.len() as u32 * font_size;
                    }

                    let surface = context
                        .font
                        .borrow_mut()
                        .render(text.as_str())
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
                    if overflow.len() == 0 {
                        break;
                    }
                }
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
