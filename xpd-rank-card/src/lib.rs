#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

pub mod colors;
mod font;
mod toy;

pub use font::Font;
pub use toy::Toy;

use resvg::usvg::{ImageKind, ImageRendering, TreeParsing, TreeTextToPath};
use std::sync::Arc;
use tera::Value;

#[derive(serde::Serialize)]
pub struct Context {
    pub level: u64,
    pub rank: i64,
    pub name: String,
    pub discriminator: String,
    pub percentage: u64,
    pub current: u64,
    pub needed: u64,
    pub font: Font,
    pub colors: crate::colors::Colors,
    pub toy: Option<Toy>,
    pub avatar: String,
}

#[derive(Clone)]
pub struct SvgState {
    fonts: Arc<resvg::usvg::fontdb::Database>,
    tera: Arc<tera::Tera>,
    threads: Arc<rayon::ThreadPool>,
}

impl SvgState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// # Errors
    /// Errors on SVG library failure
    pub async fn render(&self, context: Context) -> Result<Vec<u8>, Error> {
        let context = tera::Context::from_serialize(context)?;
        let cloned_self = self.clone();
        let (send, recv) = tokio::sync::oneshot::channel();
        self.threads.spawn(move || {
            send.send(cloned_self.do_render(&context)).ok();
        });
        recv.await?
    }
    fn do_render(&self, context: &tera::Context) -> Result<Vec<u8>, Error> {
        let svg = self.tera.render("svg", context)?;
        let resolve_data = Box::new(
            |mime: &str, data: std::sync::Arc<Vec<u8>>, _: &resvg::usvg::Options| match mime {
                "image/png" => Some(ImageKind::PNG(data)),
                "image/jpg" | "image/jpeg" => Some(ImageKind::JPEG(data)),
                _ => None,
            },
        );
        let resolve_string = Box::new(move |href: &str, _: &resvg::usvg::Options| {
            Some(ImageKind::PNG(
                Toy::from_filename(href)?.png().to_vec().into(),
            ))
        });
        let opt = resvg::usvg::Options {
            image_href_resolver: resvg::usvg::ImageHrefResolver {
                resolve_data,
                resolve_string,
            },
            image_rendering: ImageRendering::OptimizeSpeed,
            ..Default::default()
        };
        let mut tree = resvg::usvg::Tree::from_str(&svg, &opt)?;
        tree.convert_text(&self.fonts);
        let pixmap_size = tree.size.to_screen_size();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .ok_or(Error::PixmapCreation)?;
        resvg::render(
            &tree,
            resvg::FitTo::Original,
            resvg::tiny_skia::Transform::default(),
            pixmap.as_mut(),
        );
        Ok(pixmap.encode_png()?)
    }
}

impl Default for SvgState {
    fn default() -> Self {
        let mut fonts = resvg::usvg::fontdb::Database::new();
        fonts.load_font_data(include_bytes!("resources/fonts/Mojang.ttf").to_vec());
        fonts.load_font_data(include_bytes!("resources/fonts/Roboto.ttf").to_vec());
        fonts.load_font_data(include_bytes!("resources/fonts/JetBrainsMono.ttf").to_vec());
        fonts.load_font_data(include_bytes!("resources/fonts/MontserratAlt1.ttf").to_vec());
        let mut tera = tera::Tera::default();
        tera.autoescape_on(vec!["svg", "html", "xml", "htm"]);
        tera.add_raw_template("svg", include_str!("resources/card.svg"))
            .expect("Failed to build card.svg template!");
        tera.register_filter("integerhumanize", ihumanize);
        let threads = rayon::ThreadPoolBuilder::new().build().unwrap();
        Self {
            fonts: Arc::new(fonts),
            tera: Arc::new(tera),
            threads: Arc::new(threads),
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn ihumanize(v: &Value, _hm: &std::collections::HashMap<String, Value>) -> tera::Result<Value> {
    let num = if let Value::Number(num) = v {
        if let Some(num) = num.as_f64() {
            num
        } else {
            return Ok(v.clone());
        }
    } else {
        return Ok(v.clone());
    };
    let suffix = if (1_000.0..1_000_000.0).contains(&num) {
        "k"
    } else if (1_000_000.0..1_000_000_000.0).contains(&num) {
        "m"
    } else if (1_000_000_000.0..1_000_000_000_000.0).contains(&num) {
        "b"
    } else {
        ""
    };
    Ok(Value::String(format!("{num}{suffix}")))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tera error: {0}")]
    Template(#[from] tera::Error),
    #[error("uSVG error: {0}")]
    Usvg(#[from] resvg::usvg::Error),
    #[error("Integer parsing error: {0}!")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("Pixmap error: {0}")]
    Pixmap(#[from] png::EncodingError),
    #[error("Rayon error: {0}")]
    Rayon(#[from] rayon::ThreadPoolBuildError),
    #[error("Render result fetching error: {0}")]
    Recv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error("Pixmap Creation error!")]
    PixmapCreation,
    #[error("Invalid length! Color hex data length must be exactly 6 characters!")]
    InvalidLength,
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use crate::colors::Colors;

    use super::*;

    #[test]
    fn test_renderer() -> Result<(), Error> {
        let state = SvgState::new();
        let xp = rand::thread_rng().gen_range(0..=10_000_000);
        let data = mee6::LevelInfo::new(xp);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation
        )]
        let context = Context {
            level: data.level(),
            rank: rand::thread_rng().gen_range(0..=1_000_000),
            name: "Testy McTestington<span>".to_string(),
            discriminator: "0000".to_string(),
            percentage: (data.percentage() * 100.0).round() as u64,
            current: xp,
            needed: mee6::xp_needed_for_level(data.level() + 1),
            font: Font::Roboto,
            colors: Colors::default(),
            toy: Some(Toy::Parrot),
            avatar: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAQAAAAEABAMAAACuXLVVAAAAIGNIUk0AAHomAACAhAAA+gAAAIDoAAB1MAAA6mAAADqYAAAXcJy6UTwAAAAYUExURXG0zgAAAFdXV6ampoaGhr6zpHxfQ2VPOt35dJcAAAABYktHRAH/Ai3eAAAAB3RJTUUH5wMDFSE5W/eo1AAAAQtJREFUeNrt1NENgjAUQFFXYAVWYAVXcAVXYH0hoQlpSqGY2Dae82WE9971x8cDAAAAAAAAAAAAAAAAAADgR4aNAAEC/jNgPTwuBAgQ8J8B69FpI0CAgL4DhozczLgjQICAPgPCkSkjtXg/I0CAgD4Dzg4PJ8YEAQIE9BEQLyg5cEWYFyBAQHsBVxcPN8U7BAgQ0FbAlcNhcLohjkn+egECBFQPKPE8cXpQgAABzQXkwsIfUElwblaAAAF9BeyP3Z396rgAAQJ+EvCqTIAAAfUD3pUJECCgvYB5kfp89N28yR3J7RQgQED9gPjhfmG8/Oh56r1UYOpdAQIEtBFwtLBUyY7wrgABAqoHfABW2cbX3ElRgQAAACV0RVh0ZGF0ZTpjcmVhdGUAMjAyMy0wMy0wM1QyMTozMzo1NiswMDowMNpnAp0AAAAldEVYdGRhdGU6bW9kaWZ5ADIwMjMtMDMtMDNUMjE6MzM6NTYrMDA6MDCrOrohAAAAKHRFWHRkYXRlOnRpbWVzdGFtcAAyMDIzLTAzLTAzVDIxOjMzOjU3KzAwOjAwWliQSgAAAABJRU5ErkJggg==".to_string(),
        };
        let output = state.do_render(&tera::Context::from_serialize(context)?)?;
        std::fs::write("renderer_test.png", output).unwrap();
        Ok(())
    }
}
