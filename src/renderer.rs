use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;

use std::iter::repeat;
use std::ops::RangeBounds;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use async_recursion::async_recursion;

use html5ever::tendril::TendrilSink;

use hyper::Uri;

use sdl2::image::{LoadSurface, LoadTexture};

use sdl2::libc::printf;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, TextureCreator, WindowCanvas};
use sdl2::surface::Surface;
use sdl2::ttf::FontStyle;
use sdl2::video::WindowContext;

use std::string::String;

use rcdom::{Handle, NodeData};

use crate::colorscheme::ColorScheme;
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
    pub color_scheme: ColorScheme,
    pub indices: (u32, u32),
}
#[async_recursion(?Send)]
pub async fn render<'a>(
    indent: usize,
    handle: &Handle,
    tag_name: &str,
    context: &'a mut RendererContext,
) {
    let node = handle;
    let mut next_tag_name = "";
    let invisible_tags = [
        "style", "script", "head", "title", "meta", "link", "img", "br",
    ];
    match node.data {
        NodeData::Text { ref contents } => {
            // println!("<>{}</>", contents.borrow());
            if tag_name == "title" {
                context
                    .canvas
                    .borrow_mut()
                    .window_mut()
                    .set_title(&contents.borrow())
                    .unwrap();
            }
            if tag_name == "br" {
                context.indices.1 += 12;
            }

            if &contents.borrow().trim().len() != &0 && !invisible_tags.contains(&tag_name) {
                let mut overflow = String::new();
                loop {
                    let mut text_color = context.color_scheme.text;
                    let mut text = contents.borrow().to_string().replace("\n", "");
                    if overflow.len() > 0 {
                        text = overflow;
                        overflow = String::new();
                    }
                    let (mut width, mut height) = context.font.borrow_mut().size_of(&text).unwrap();
                    let mut font_size = 16 * context.scaling_factor;
                    if tag_name == "a" {
                        context.font.borrow_mut().set_style(FontStyle::UNDERLINE);
                        text_color = context.color_scheme.link;
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
                    let mut c1 = 0;
                    let c2 =
                        context.canvas.borrow_mut().window().size().0 * context.scaling_factor * 2;
                    let mut fitting_text = String::new();
                    for (i, word) in text.split_whitespace().enumerate() {
                        fitting_text.push_str(word);
                        fitting_text.push(' ');
                        // println!("{} < {}", c1, c2);
                        // println!("<{}>", fitting_text);
                        c1 += font_size * word.len() as u32;
                        if c1 > c2 {
                            overflow = text
                                .split_whitespace()
                                .skip(i)
                                .collect::<Vec<_>>()
                                .join(" ");
                            fitting_text.truncate(
                                fitting_text.len()
                                    - fitting_text.split_whitespace().last().unwrap().len()
                                    - 1,
                            );
                            break;
                        }
                    }
                    text = fitting_text;
                    width = context.font.borrow_mut().size_of(&text).unwrap().0;
                    width = (width as f32 * ratio).ceil() as u32;
                    // if text.contains("\n") {
                    //     overflow = text.split("\n").last().unwrap().to_string();
                    //     text.truncate(text.len() - overflow.len());
                    // }
                    // println!("==== START ====");
                    // while c1 > c2 {
                    //     // overflow.insert(0, text.pop().unwrap());
                    //     // overflow.insert_str(
                    //     //     0,
                    //     //     &(text.split_whitespace().last().unwrap().to_owned() + &" ".to_owned()),
                    //     // );
                    //     overflow +=
                    //         &(text.split_whitespace().last().unwrap().to_owned() + &" ".to_owned());
                    //     println!("<{:?}>", overflow);
                    //     if text.len() as i32 - overflow.len() as i32 > 1 {
                    //         text.truncate(text.len() - overflow.len());
                    //     }
                    //     width = context.font.borrow_mut().size_of(&text).unwrap().0;
                    //     width = (width as f32 * ratio).ceil() as u32;
                    //     c1 = text.len() as u32 * font_size;
                    // }
                    // println!("==== END ====");

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
                        (context.indices.0 as i32 + context.viewport.0),
                        context.indices.1 as i32 + context.viewport.1,
                        width,
                        height,
                        || println!("Hello, I'm text"),
                    ));
                    context.indices.0 = indent as u32 * 12;

                    context
                        .canvas
                        .borrow_mut()
                        .copy(
                            &texture,
                            None,
                            rect!(
                                context.indices.0 as i32 + context.viewport.0,
                                context.indices.1 as i32 + context.viewport.1,
                                width,
                                height
                            ),
                        )
                        .unwrap();
                    context.font.borrow_mut().set_style(FontStyle::NORMAL);
                    context.indices.1 += height as u32;
                    // println!("#text");
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
            // println!("{}", &name.local);

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
                    context.indices.0 as i32 + context.viewport.0,
                    context.indices.1 as i32 + context.viewport.1,
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
                            context.indices.0 as i32 + context.viewport.0,
                            context.indices.1 as i32 + context.viewport.1,
                            width,
                            height
                        ),
                    )
                    .unwrap();
                context.indices.1 += height;
            }
        }
        NodeData::ProcessingInstruction { .. } => unreachable!(),
        _ => {}
    }
    for child in node.children.borrow().iter() {
        render(indent + 1, child, next_tag_name, context).await;
    }
}

pub fn print_dom(indent: usize, handle: &Handle) {
    let node = handle;
    // FIXME: don't allocate
    print!("{}", repeat(" ").take(indent).collect::<String>());
    match node.data {
        NodeData::Document => println!("#Document"),

        NodeData::Doctype {
            ref name,
            ref public_id,
            ref system_id,
        } => println!("<!DOCTYPE {} \"{}\" \"{}\">", name, public_id, system_id),

        NodeData::Text { ref contents } => {
            println!("#text: {}", contents.borrow().escape_default())
        }

        NodeData::Comment { ref contents } => println!("<!-- {} -->", contents.escape_default()),

        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            assert!(name.ns == ns!(html));
            print!("<{}", name.local);
            for attr in attrs.borrow().iter() {
                assert!(attr.name.ns == ns!());
                print!(" {}=\"{}\"", attr.name.local, attr.value);
            }
            println!(">");
        }

        NodeData::ProcessingInstruction { .. } => unreachable!(),
    }

    for child in node.children.borrow().iter() {
        print_dom(indent + 4, child);
    }
}
