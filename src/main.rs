// Copyright (c) 2025 Philip Taylor
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{fs::File, collections::BTreeMap, cmp::Ordering, io::Cursor};

use eframe::{
    egui::{self, Sense},
    epaint::{Color32, PathShape, Pos2, Rect, Shape, Stroke, Vec2, FontId, FontFamily},
};
use egui::{epaint::{CircleShape, PathStroke}, ColorImage};
use egui_extras::RetainedImage;
use image::RgbImage;
use serde::{Deserialize, Serialize};

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_maximized(true),
        ..Default::default()
    };
    eframe::run_native(
        "Annotator",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}

#[derive(Clone)]
struct Line {
    text: String,
    points: Vec<Vec2>,
    bbox: Rect,
    left: f32,
    mid: Vec2,
}

#[derive(Serialize, Deserialize)]
struct Article {
    polys: Vec<Vec<Pos2>>,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct Page {
    date: Option<String>,
    summary: Option<String>,
    articles: Vec<Article>,
}

#[derive(Serialize, Deserialize)]
struct State {
    images: Vec<String>,
    pages: BTreeMap<String, Page>,
    open_image: usize,
}

struct MyApp {
    runtime: tokio::runtime::Runtime,

    image: RgbImage,
    retained_image: RetainedImage,

    crop_image: RgbImage,
    retained_crop: RetainedImage,

    vertexes: Vec<Pos2>, // image-space coords
    lines: Vec<Line>,
    draft_text: String,
    offset: Vec2,

    state: State,
    open_article: Option<usize>,
}

// const ANNOTATIONS_FILENAME: &str = "annotations/annotations.yaml";
// const JPEG_PATH: &str = "../scrapbook-images/jpeg1/pages/";
// const DEFAULT_SCALE: f32 = 0.75;

// const ANNOTATIONS_FILENAME: &str = "annotations/annotations2.yaml";
// const JPEG_PATH: &str = "../scrapbook-images/jpeg2/";
// const DEFAULT_SCALE: f32 = 0.5;

const ANNOTATIONS_FILENAME: &str = "annotations/annotations3.yaml";
const JPEG_PATH: &str = "../scrapbook-images/jpeg3/";
const DEFAULT_SCALE: f32 = 0.125;

impl Default for MyApp {
    fn default() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        let mut state;
        if let Ok(file) = File::open(ANNOTATIONS_FILENAME) {
            state = serde_yaml::from_reader(file).unwrap();
        } else {
            state = State { images: Vec::new(), pages: BTreeMap::new(), open_image: 0 };
        }

        for page in state.pages.values_mut() {
            if page.date == None {
                page.date = Some(String::new());
            }
            if page.summary == None {
                page.summary = Some(String::new());
            }
        }

        let image = ColorImage::new([1, 1], Color32::BLACK);
        let mut ret = Self {
            runtime,
            image: RgbImage::new(1, 1),
            retained_image: RetainedImage::from_color_image("black", image.clone()),
            crop_image: RgbImage::new(1, 1),
            retained_crop: RetainedImage::from_color_image("black", image.clone()),
            vertexes: Vec::new(),
            lines: Vec::new(),
            draft_text: String::new(),
            offset: Vec2::ZERO,

            state,
            open_article: None,
        };
        ret.load_image();
        ret
    }
}

impl State {
    fn page(&mut self) -> &mut Page {
        self.pages.entry(self.images[self.open_image].clone()).or_insert_with(|| Page { date: Some(String::new()), summary: Some(String::new()), articles: Vec::new() })
    }
}

fn cmp_f32(a: &f32, b: &f32) -> Ordering {
    a.partial_cmp(&b).unwrap()
}

impl MyApp {
    fn load_image(&mut self) {
        let mut lines: Vec<Line> = Vec::new();

        let image = image::load_from_memory(
            std::fs::read(format!("{}{}", JPEG_PATH, self.state.images[self.state.open_image])).unwrap().as_ref()
        )
        .unwrap().to_rgb8();
        let egui_image = ColorImage::from_rgb([image.width() as _, image.height() as _], image.as_flat_samples().as_slice());
        let retained_image = RetainedImage::from_color_image("image", egui_image);

        self.lines = lines;
        self.image = image;
        self.retained_image = retained_image;
    }

    fn save(&mut self) {
        let file = File::create(ANNOTATIONS_FILENAME).unwrap();
        serde_yaml::to_writer(file, &self.state).unwrap();
    }

    fn new_article(&mut self) {
        let page = self.state.page();
        let id = page.articles.len();
        page.articles.push(Article {
            polys: Vec::new(),
            text: String::new(),
        });
        self.open_article = Some(id);
    }

    fn merge_lines(lines: Vec<Line>, image_width: f32) -> String {
        let mut text = String::new();

        let mut dehyphenating = false;
        for (i, line) in lines.iter().enumerate() {
            let mut start = 0;
            if dehyphenating {
                // Add the first word after a hyphen onto the previous line
                if let Some(space) = line.text.find(" ") {
                    text.push_str(&line.text[0..space]);
                    text.push_str("\n");
                    start = space + 1;
                }
            } else {
                // Try to detect paragraph indents
                if i > 0 && i + 1 < lines.len() {
                    let x0 = lines[i - 1].left * image_width;
                    let x1 = lines[i + 0].left * image_width;
                    let x2 = lines[i + 1].left * image_width;
                    let min = 8.0;
                    let max = 40.0;
                    if min < x1 - x0 && x1 - x0 < max && min < x1 - x2 && x1 - x2 < max {
                        text.push_str("\n");
                    }
                }
            }
            if line.text.ends_with("-") {
                text.push_str(&line.text[start..line.text.len() - 1]);
                dehyphenating = true;
            } else {
                text.push_str(&line.text[start..]);
                text.push_str("\n");
                dehyphenating = false;
            }
        }

        text
    }

    // Test if line (ox, oy)--(inf, oy) intersects (ax, ay)--(bx, by)
    fn ray_intersect(ox: f32, oy: f32, ax: f32, ay: f32, bx: f32, by: f32) -> bool {
        // Test if a,b on opposite sides of o--inf:
        if (ay - oy).signum() == (by - oy).signum() {
            return false;
        }
        // Test if o,inf on opposite sides of a--b:
        //  s0 = (ox-ax, oy-ay) . (by-ay, ax-bx)
        //  s1 = (ox+inf-ax, oy-ay) . (by-ay, ax-bx) =~ inf*(by-ay)
        let s0 = ((ox - ax) * (by - ay) + (oy - ay) * (ax - bx)).signum();
        let s1 = (by - ay).signum();
        return s0 != s1;
    }

    fn extract_image(&mut self) -> Vec<u8> {
        let x0 = self.vertexes.iter().map(|p| p.x).min_by(cmp_f32).unwrap();
        let x1 = self.vertexes.iter().map(|p| p.x).max_by(cmp_f32).unwrap();
        let y0 = self.vertexes.iter().map(|p| p.y).min_by(cmp_f32).unwrap();
        let y1 = self.vertexes.iter().map(|p| p.y).max_by(cmp_f32).unwrap();

        let margin = 4.0;
        let x0 = ((x0 - margin) as i32).clamp(0, self.image.width() as i32) as u32;
        let x1 = ((x1 + margin) as i32).clamp(0, self.image.width() as i32) as u32;
        let y0 = ((y0 - margin) as i32).clamp(0, self.image.height() as i32) as u32;
        let y1 = ((y1 + margin) as i32).clamp(0, self.image.height() as i32) as u32;

        let mut vertexes = self.vertexes.clone();
        vertexes.push(self.vertexes[0]); // close the shape
        let lines: Vec<_> = vertexes.windows(2).map(|vs| {
            (vs[0].x - x0 as f32, vs[0].y - y0 as f32, vs[1].x - x0 as f32, vs[1].y - y0 as f32)
        }).collect();

        let mut image = RgbImage::new(x1 - x0, y1 - y0);
        for (x, y, p) in image.enumerate_pixels_mut() {
            let xf = x as f32;
            let yf = y as f32;
            let crossings = lines.iter().filter(|line| {
                Self::ray_intersect(xf, yf, line.0, line.1, line.2, line.3)
            }).count();
            let inside = (crossings % 2) == 1;
            if inside {
                *p = *self.image.get_pixel(x0 + x, y0 + y);
            } else {
                *p = image::Rgb([48, 48, 48]);
            }
        }

        let egui_image = ColorImage::from_rgb([image.width() as _, image.height() as _], image.as_flat_samples().as_slice());
        self.retained_crop = RetainedImage::from_color_image("crop", egui_image);

        let mut bytes: Vec<u8> = Vec::new();
        image.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut bytes), 90)).unwrap();

        self.crop_image = image;

        bytes
    }

    async fn extract_text(&self, image_bytes: Vec<u8>) -> (String, RgbImage) {
        let config = aws_config::defaults(aws_config::BehaviorVersion::v2024_03_28()).region("eu-west-2").load().await;
        let client = aws_sdk_textract::Client::new(&config);

        let res = client
            .detect_document_text()
            .document(aws_sdk_textract::types::Document::builder().bytes(aws_sdk_textract::primitives::Blob::new(image_bytes)).build())
            .send()
            .await;

        match res {
            Ok(doc) => {
                let mut lines: Vec<Line> = Vec::new();

                for block in doc.blocks() {
                    if *block.block_type().unwrap() == aws_sdk_textract::types::BlockType::Line {
                        let points: Vec<_> = block.geometry().unwrap().polygon()
                            .iter()
                            .map(|pt| {
                                Vec2::new(pt.x(), pt.y())
                            })
                            .collect();

                        let bbox = block.geometry().unwrap().bounding_box().unwrap();

                        let mid = Vec2::new(bbox.left() + bbox.width() / 2.0, bbox.top() + bbox.height() / 2.0);
                        let left = bbox.left();

                        lines.push(Line {
                            text: block.text().unwrap().to_string(),
                            bbox: Rect::from_min_size(Pos2::new(bbox.left(), bbox.top()), Vec2::new(bbox.width(), bbox.height())),
                            points,
                            left,
                            mid,
                        });
                    }
                }

                // Sort top-to-bottom, with a fudge for simple cases where a line is split into multiple Lines
                // and we want to do them left-to-right
                lines.sort_by(|a, b| {
                    let am = a.mid.y + a.left / 40.0;
                    let bm = b.mid.y + b.left / 40.0;
                    am.partial_cmp(&bm).unwrap()
                });

                return (Self::merge_lines(lines, self.retained_crop.width() as f32), self.crop_image.clone());
            },
            Err(err) => {
                return (format!("Error: {:?}", err), self.crop_image.clone());
            }
        }
    }
}

struct Scaler {
    scale: f32, // screen-space units per image-space pixel
    viewport: Vec2, // size in screen-space
    offset: Vec2, // screen-space coords
    image_rect: Rect, // screen-space coords of viewport
}

impl Scaler {
    fn screen_to_image(&self, screen: Pos2) -> Pos2 {
        ((screen.to_vec2() - self.image_rect.left_top().to_vec2() + self.offset) / self.scale).to_pos2()
    }

    fn image_to_screen(&self, image: Pos2) -> Pos2 {
        ((image.to_vec2() * self.scale) - self.offset + self.image_rect.left_top().to_vec2()).to_pos2()
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(2.0);

        egui::CentralPanel::default().show(ctx, |ui| {
            let scale = DEFAULT_SCALE;
            let viewport = Vec2::new(1920.0, 1080.0 - 48.0);

            let show_boxes = !ui.input(|i| i.modifiers.alt);

            let response = ui.allocate_response(viewport, Sense::click_and_drag());
            let image_rect = response.rect;

            let scaler = Scaler {
                scale,
                viewport,
                offset: self.offset,
                image_rect,
            };

            let mut mesh = egui::Mesh::with_texture(self.retained_image.texture_id(ctx));
            mesh.add_rect_with_uv(
                image_rect,
                Rect::from_min_max(
                    (self.offset / (self.retained_image.size_vec2() * scale)).to_pos2(),
                    ((self.offset + viewport) / (self.retained_image.size_vec2() * scale)).to_pos2(),
                ),
                Color32::WHITE,
            );
            ui.painter().add(Shape::mesh(mesh));

            if show_boxes {
                for article in &self.state.page().articles {
                    for vertexes in &article.polys {
                        // egui assumes convex, which is not true
                        let path = PathShape {
                            points: vertexes.iter().map(|&p| scaler.image_to_screen(p)).collect(),
                            closed: true,
                            fill: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                            stroke: PathStroke::NONE,
                        };
                        ui.painter().add(path);
                    }
                }
            }

            if response.dragged_by(egui::PointerButton::Secondary) {
                self.offset -= response.drag_delta();
            }

            if !self.vertexes.is_empty() && response.clicked_by(egui::PointerButton::Middle) {
                self.vertexes.pop();
            }

            if response.clicked_by(egui::PointerButton::Primary) {
                if !ctx.input(|i| i.modifiers.shift) {
                    self.vertexes.clear();
                }

                self.vertexes.push(scaler.screen_to_image(response.interact_pointer_pos().unwrap()));
            }

            let adding_vertex = !self.vertexes.is_empty() && ctx.input(|i| i.modifiers.shift);
            let mut temp_vertex = false;
            if adding_vertex {
                if let Some(p) = response.hover_pos() {
                    self.vertexes.push(scaler.screen_to_image(p));
                    temp_vertex = true;
                }
            }

            if show_boxes {
                for &vertex in &self.vertexes {
                    ui.painter().add(Shape::Circle(
                        CircleShape {
                            center: scaler.image_to_screen(vertex),
                            radius: 3.0,
                            fill: Color32::TRANSPARENT,
                            stroke: Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 0, 0, 255))
                        }
                    ));
                }
                ui.painter().add(Shape::Path(
                    PathShape {
                        points: self.vertexes.iter().map(|&p| scaler.image_to_screen(p)).collect(),
                        closed: !adding_vertex,
                        fill: Color32::TRANSPARENT,
                        stroke: PathStroke::new(2.0, Color32::from_rgba_unmultiplied(255, 0, 0, 255))
                    }
                ));
            }

            if temp_vertex {
                self.vertexes.pop();
            }

            if self.vertexes.len() >= 4 {
                let x1 = self.vertexes.iter().map(|p| p.x).max_by(|a, b| a.partial_cmp(&b).unwrap()).unwrap();
                let y0 = self.vertexes.iter().map(|p| p.y).min_by(|a, b| a.partial_cmp(&b).unwrap()).unwrap();

                ui.allocate_ui_at_rect(
                    Rect::from_min_size(
                        scaler.image_to_screen(Pos2::new(x1 + 20.0, y0 - 20.0)),
                        Vec2::new(500.0, 200.0),
                    ),
                    |ui| {
                        self.popup(ui);
                    },
                );
            }

            ui.allocate_ui_at_rect(
                Rect::from_min_max(Pos2::new(viewport.x - 400.0, 0.0), viewport.to_pos2()),
                |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(192))
                        .show(ui, |ui| {
                            self.sidebar(scaler, ui);
                        });
                },
            );
        });
    }
}

impl MyApp {
    fn popup(&mut self, ui: &mut egui::Ui) {
        let draft_font = FontId::new(11.0, FontFamily::Monospace);

        egui::Frame::none()
            .fill(egui::Color32::BLACK)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Extract").clicked() {
                            let image = self.extract_image();
                            (self.draft_text, self.crop_image) = self.runtime.block_on(self.extract_text(image));

                            let egui_image = ColorImage::from_rgb([self.crop_image.width() as _, self.crop_image.height() as _], self.crop_image.as_flat_samples().as_slice());
                            self.retained_crop = RetainedImage::from_color_image("crop", egui_image);
                        }
                        let articles = &mut self.state.page().articles;
                        if ui.button("Append").clicked() {
                            if let Some(i) = self.open_article {
                                articles[i].text.push_str(&self.draft_text.trim_end());
                                articles[i].text.push_str("\n");
                                articles[i].polys.push(self.vertexes.clone());
                            }
                        }
                        if ui.button("Append P").clicked() {
                            if let Some(i) = self.open_article {
                                articles[i].text.push_str("\n");
                                articles[i].text.push_str(&self.draft_text.trim_end());
                                articles[i].text.push_str("\n");
                                articles[i].polys.push(self.vertexes.clone());
                            }
                        }
                        if ui.button("#").clicked() {
                            self.draft_text = self.draft_text.replace("\n", " ").trim().to_string() + "\n";
                            self.draft_text.insert_str(0, "# ");
                        }
                        // if ui.button("##").clicked() {
                        //     self.draft_text = self.draft_text.replace("\n", " ").trim().to_string() + "\n";
                        //     self.draft_text.insert_str(0, "## ");
                        // }
                        if ui.button("Article").clicked() {
                            self.new_article();
                        }
                    });

                    // ui.image(self.retained_crop.texture_id(ctx), self.retained_crop.size_vec2() * scale * 0.5);
                    ui.add(egui::TextEdit::multiline(&mut self.draft_text).font(draft_font.clone()).desired_width(400.0));
                });
            });
    }

    fn sidebar(&mut self, scaler: Scaler, ui: &mut egui::Ui) {
        let article_font = FontId::new(10.0, FontFamily::Proportional);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(self.state.open_image > 0, egui::Button::new("<<")).clicked() {
                    self.state.open_image = self.state.open_image.saturating_sub(10);
                    self.open_article = None;
                    self.load_image();
                }
                if ui.add_enabled(self.state.open_image > 0, egui::Button::new("<")).clicked() {
                    self.state.open_image -= 1;
                    self.open_article = None;
                    self.load_image();
                }
                let mut open_image = self.state.open_image.to_string();
                if ui.add(egui::TextEdit::singleline(&mut open_image).desired_width(30.0)).changed() {
                    if let Ok(open_image) = open_image.parse::<usize>() {
                        self.state.open_image = open_image.clamp(0, self.state.images.len() - 1);
                        self.open_article = None;
                        self.load_image();
                    }
                }
                if ui.add_enabled(self.state.open_image + 1 < self.state.images.len(), egui::Button::new(">")).clicked() {
                    self.state.open_image += 1;
                    self.open_article = None;
                    self.load_image();
                }
                if ui.add_enabled(self.state.open_image + 1 < self.state.images.len(), egui::Button::new(">>")).clicked() {
                    self.state.open_image = usize::min(self.state.images.len() - 1, self.state.open_image + 10);
                    self.open_article = None;
                    self.load_image();
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("New article").clicked() {
                    self.new_article();
                }
                let can_delete = match self.open_article {
                    Some(i) => self.state.page().articles[i].text.is_empty(),
                    None => false,
                };
                if ui.add_enabled(can_delete, egui::Button::new("Delete article")).clicked() {
                    self.state.page().articles.remove(self.open_article.unwrap());
                    self.open_article = None;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Date");
                ui.text_edit_singleline(self.state.page().date.as_mut().unwrap());
            });

            ui.horizontal(|ui| {
                ui.label("Summary");
                ui.text_edit_singleline(self.state.page().summary.as_mut().unwrap());
            });

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut insert_note = None;
                for (article_id, article) in self.state.page().articles.iter_mut().enumerate() {

                    if ui.button("+N").clicked() {
                        insert_note = Some(article_id);
                    }

                    let res = egui::CollapsingHeader::new(format!(
                        "({}) {}...",
                        article_id,
                        article.text.replace("\n", " ").chars().take(40).collect::<String>()
                    ))
                    .id_salt(("article", article_id))
                    .open(Some(self.open_article == Some(article_id)))
                    .show(ui, |ui| {
                        let mut del = None;
                        for (i, vertexes) in article.polys.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button("-").clicked() {
                                    del = Some(i);
                                }
                                if ui.label(format!("{:?}", vertexes)).hovered() {
                                    let path = PathShape {
                                        points: vertexes.iter().map(|&p| scaler.image_to_screen(p)).collect(),
                                        closed: true,
                                        fill: Color32::TRANSPARENT,
                                        stroke: PathStroke::new(1.0, Color32::from_rgba_unmultiplied(0, 255, 0, 255))
                                    };
                                    ui.painter().add(path);
                                }
                            });
                        }
                        if let Some(d) = del {
                            article.polys.remove(d);
                        }
                        ui.add(egui::TextEdit::multiline(&mut article.text).font(article_font.clone()));
                    });

                    if res.header_response.clicked() {
                        if self.open_article == Some(article_id) {
                            self.open_article = None;
                        } else {
                            self.open_article = Some(article_id);
                        }
                    }
                }

                if let Some(article_id) = insert_note {
                    self.state.page().articles.insert(article_id, Article {
                        polys: Vec::new(),
                        text: String::from("[NOTE] "),
                    });
                    self.open_article = Some(article_id);
                }

                ui.allocate_space(ui.available_size());
            });
        });
    }
}
