use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use eframe::egui;

pub struct AnimatedGif {
    frames: Vec<egui::ColorImage>,
    delays: Vec<Duration>,
    textures: Vec<Option<egui::TextureHandle>>,
}

pub struct GifHandler {
    gif: Option<AnimatedGif>,
    current_frame: usize,
    last_frame_time: Instant,
    gif_load_id: u64,
    current_path: Option<PathBuf>,
}

impl GifHandler {
    pub fn new() -> Self {
        Self {
            gif: None,
            current_frame: 0,
            last_frame_time: Instant::now(),
            gif_load_id: 0,
            current_path: None,
        }
    }

    pub fn load_from_path(&mut self, path: PathBuf) -> bool {
        if let Ok((gif_data, _dims)) = load_gif_data(&path) {
            self.gif = Some(gif_data);
            self.current_frame = 0;
            self.gif_load_id += 1;
            self.current_path = Some(path);
            true
        } else {
            false
        }
    }
    
    // Eagerly loads all frames into textures.
    pub fn prime_cache(&mut self, ctx: &eframe::egui::Context) {
        if let Some(gif) = &mut self.gif {
            for i in 0..gif.frames.len() {
                if gif.textures[i].is_none() {
                     let frame_data = gif.frames[i].clone();
                     let name = format!("gif_{}_frame_{}", self.gif_load_id, i);
                     gif.textures[i] = Some(ctx.load_texture(name, frame_data, Default::default()));
                }
            }
        }
    }

    pub fn tick(&mut self, ctx: &eframe::egui::Context) {
        if let Some(gif) = &mut self.gif {
            // Lazy loading in case cache wasn't primed
            if gif.textures.get(self.current_frame).and_then(|t| t.as_ref()).is_none() {
                let frame_data = gif.frames[self.current_frame].clone();
                let name = format!("gif_{}_frame_{}", self.gif_load_id, self.current_frame);
                gif.textures[self.current_frame] = Some(ctx.load_texture(name, frame_data, Default::default()));
            }

            let delay = gif.delays.get(self.current_frame).copied().unwrap_or_default();
            if self.last_frame_time.elapsed() >= delay {
                self.current_frame = (self.current_frame + 1) % gif.frames.len();
                self.last_frame_time = Instant::now();
            }
        }
    }

    pub fn draw_background(&self, ctx: &eframe::egui::Context) {
        if let Some(gif) = &self.gif {
            if let Some(texture) = gif.textures.get(self.current_frame).and_then(|t| t.as_ref()) {
                egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
                    let available = ui.max_rect();
                    let img_size = texture.size_vec2();
                    if img_size.y == 0.0 { return; }

                    let ratio = (available.width() / img_size.x).min(available.height() / img_size.y);
                    let new_size = img_size * ratio;

                    let image_rect = egui::Rect::from_center_size(available.center(), new_size);
                    ui.painter().image(
                        texture.id(),
                        image_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                });
            }
        }
    }

    pub fn get_path_string(&self) -> Option<String> {
        self.current_path.as_ref().and_then(|p| p.to_str().map(String::from))
    }
}

pub fn get_gif_dimensions(path: &Path) -> Result<(u16, u16), Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let decoder = gif::DecodeOptions::new();
    let reader = decoder.read_info(file)?;
    Ok((reader.width(), reader.height()))
}

fn load_gif_data(path: &Path) -> Result<(AnimatedGif, (u16, u16)), Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(file)?;

    let (width, height) = (decoder.width(), decoder.height());
    let mut frames = Vec::new();
    let mut delays = Vec::new();
    let mut canvas = vec![0; width as usize * height as usize * 4];
    let mut previous_canvas: Option<Vec<u8>> = None;

    while let Some(frame) = decoder.read_next_frame()? {
        delays.push(Duration::from_millis(frame.delay as u64 * 10));
        if frame.dispose == gif::DisposalMethod::Previous {
            previous_canvas = Some(canvas.clone());
        }

        for y in 0..frame.height {
            for x in 0..frame.width {
                let frame_idx = (y as usize * frame.width as usize + x as usize) * 4;
                if frame.buffer[frame_idx + 3] == 0 { continue; }
                let (canvas_x, canvas_y) = (frame.left + x, frame.top + y);
                if canvas_x < width && canvas_y < height {
                    let canvas_idx = (canvas_y as usize * width as usize + canvas_x as usize) * 4;
                    canvas[canvas_idx..canvas_idx + 4].copy_from_slice(&frame.buffer[frame_idx..frame_idx + 4]);
                }
            }
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &canvas));

        match frame.dispose {
            gif::DisposalMethod::Background => {
                for y in frame.top..frame.top + frame.height {
                    for x in frame.left..frame.left + frame.width {
                        if x < width && y < height {
                            let idx = (y as usize * width as usize + x as usize) * 4;
                            canvas[idx..idx + 4].copy_from_slice(&[0, 0, 0, 0]);
                        }
                    }
                }
            },
            gif::DisposalMethod::Previous => {
                if let Some(prev) = previous_canvas.take() { canvas = prev; }
            },
            _ => (),
        }
    }

    let num_frames = frames.len();
    if num_frames == 0 { return Err("GIF contains no frames".into()); }
    Ok((
        AnimatedGif { frames, delays, textures: vec![None; num_frames] },
        (width, height),
    ))
}