use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use image::ImageFormat;
use pdfium_render::prelude::*;

#[derive(Debug, Clone)]
pub struct ParsePdfOptions {
    pub export_images: bool,
    pub max_pages: Option<usize>,
}

impl Default for ParsePdfOptions {
    fn default() -> Self {
        Self {
            export_images: true,
            max_pages: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsePdfResult {
    pub markdown: String,
    pub image_paths: Vec<PathBuf>,
    pub page_count: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
enum PdfPageContent {
    Text(String),
    Image { alt: String, path: String },
}

#[derive(Debug, Clone)]
struct PdfPageBlock {
    top: f32,
    left: f32,
    content: PdfPageContent,
}

pub fn parse_pdf_to_markdown(
    pdf_path: &Path,
    options: ParsePdfOptions,
) -> Result<ParsePdfResult, String> {
    if !pdf_path.exists() || !pdf_path.is_file() {
        return Err(format!(
            "Path does not exist or is not a file: {}",
            pdf_path.display()
        ));
    }

    let pdfium = create_pdfium()?;
    let document = pdfium
        .load_pdf_from_file(pdf_path, None)
        .map_err(|e| format!("Failed to open PDF: {}", e))?;

    let total_pages = document.pages().len() as usize;
    let page_limit = options
        .max_pages
        .and_then(|value| if value == 0 { None } else { Some(value) })
        .unwrap_or(total_pages)
        .min(total_pages);
    let truncated = page_limit < total_pages;
    let image_output_dir = build_pdf_image_output_dir(pdf_path);

    let mut all_image_paths = Vec::new();
    let mut markdown_sections = Vec::new();
    let mut image_output_dir_ready = false;

    for (page_index, page) in document.pages().iter().enumerate().take(page_limit) {
        let mut blocks = Vec::new();
        let mut page_image_index = 0usize;

        if let Ok(page_text) = page.text() {
            let mut segment_texts = Vec::new();
            for segment in page_text.segments().iter() {
                let bounds = segment.bounds();
                let text = normalize_pdf_text(&segment.text());
                if text.is_empty() {
                    continue;
                }

                segment_texts.push(text.clone());
                blocks.push(PdfPageBlock {
                    top: bounds.top().value,
                    left: bounds.left().value,
                    content: PdfPageContent::Text(text),
                });
            }

            if looks_like_fragmented_segments(&segment_texts) {
                blocks.retain(|block| !matches!(block.content, PdfPageContent::Text(_)));
                let page_text_all = normalize_pdf_text(&page_text.all());
                if !page_text_all.is_empty() {
                    blocks.push(PdfPageBlock {
                        top: f32::MAX / 4.0,
                        left: 0.0,
                        content: PdfPageContent::Text(page_text_all),
                    });
                }
            }
        }

        for object in page.objects().iter() {
            let (top, left) = object
                .bounds()
                .map(|bounds| (bounds.top().value, bounds.left().value))
                .unwrap_or((0.0, 0.0));

            if !options.export_images {
                continue;
            }

            if let Some(image_object) = object.as_image_object() {
                if !image_output_dir_ready {
                    std::fs::create_dir_all(&image_output_dir)
                        .map_err(|e| format!("Failed to create image output directory: {}", e))?;
                    image_output_dir_ready = true;
                }

                let image = image_object
                    .get_processed_image(&document)
                    .or_else(|_| image_object.get_raw_image())
                    .map_err(|e| {
                        format!("Failed to decode image on page {}: {}", page_index + 1, e)
                    })?;

                page_image_index += 1;
                let file_name = format!(
                    "page-{:03}-image-{:03}.png",
                    page_index + 1,
                    page_image_index
                );
                let image_path = image_output_dir.join(file_name);

                image
                    .save_with_format(&image_path, ImageFormat::Png)
                    .map_err(|e| format!("Failed to save image extracted from PDF: {}", e))?;

                all_image_paths.push(image_path.clone());
                blocks.push(PdfPageBlock {
                    top,
                    left,
                    content: PdfPageContent::Image {
                        alt: format!("page {} image {}", page_index + 1, page_image_index),
                        path: markdown_image_reference_path(pdf_path, &image_path),
                    },
                });
            }
        }

        blocks.sort_by(|a, b| {
            b.top
                .partial_cmp(&a.top)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.left.partial_cmp(&b.left).unwrap_or(Ordering::Equal))
        });

        let mut lines = Vec::new();
        lines.push(format!("## Page {}", page_index + 1));

        if blocks.is_empty() {
            lines.push("_No extractable content found._".to_string());
        } else {
            for block in blocks {
                match block.content {
                    PdfPageContent::Text(text) => lines.push(text),
                    PdfPageContent::Image { alt, path } => {
                        lines.push(format!("![{}](<{}>)", alt, path))
                    }
                }
            }
        }

        markdown_sections.push(lines.join("\n"));
    }

    Ok(ParsePdfResult {
        markdown: markdown_sections.join("\n\n"),
        image_paths: all_image_paths,
        page_count: total_pages as u32,
        truncated,
    })
}

fn create_pdfium() -> Result<Pdfium, String> {
    let mut attempts = Vec::<String>::new();

    if let Ok(custom_library_path) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
        let trimmed = custom_library_path.trim();
        if !trimmed.is_empty() {
            match Pdfium::bind_to_library(PathBuf::from(trimmed)) {
                Ok(bindings) => return Ok(Pdfium::new(bindings)),
                Err(error) => {
                    attempts.push(format!("PDFIUM_DYNAMIC_LIB_PATH='{}': {}", trimmed, error))
                }
            }
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for candidate_dir in candidate_library_dirs(exe_dir) {
                let library_path = Pdfium::pdfium_platform_library_name_at_path(&candidate_dir);
                match Pdfium::bind_to_library(library_path.clone()) {
                    Ok(bindings) => return Ok(Pdfium::new(bindings)),
                    Err(error) => attempts.push(format!("{}: {}", library_path.display(), error)),
                }
            }
        }
    }

    match Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")) {
        Ok(bindings) => return Ok(Pdfium::new(bindings)),
        Err(error) => attempts.push(format!("current dir: {}", error)),
    }

    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(error) => {
            attempts.push(format!("system library: {}", error));
            let diagnostics = attempts.join(" | ");
            eprintln!(
                "[pdf_parse] failed to load PDFium runtime library. Attempts: {}",
                diagnostics
            );
            Err(format!(
                "Failed to load PDFium runtime library. Place pdfium.dll near the app, in resources, in PATH, or set PDFIUM_DYNAMIC_LIB_PATH. Attempts: {}",
                diagnostics
            ))
        }
    }
}

fn candidate_library_dirs(exe_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![exe_dir.to_path_buf()];

    let resource_subdir = exe_dir.join("resources");
    dirs.push(resource_subdir.clone());
    dirs.push(resource_subdir.join("pdfium"));
    dirs.push(exe_dir.join("Resources"));
    dirs.push(exe_dir.join("Resources").join("pdfium"));

    if let Some(parent) = exe_dir.parent() {
        dirs.push(parent.join("Resources"));
        dirs.push(parent.join("Resources").join("pdfium"));
    }

    let mut deduped = Vec::new();
    for dir in dirs {
        if deduped.iter().any(|existing: &PathBuf| existing == &dir) {
            continue;
        }
        deduped.push(dir);
    }
    deduped
}

fn build_pdf_image_output_dir(pdf_path: &Path) -> PathBuf {
    let parent = pdf_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = pdf_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    parent.join(format!("{}-pdf-images", sanitize_name_for_path(stem)))
}

fn sanitize_name_for_path(input: &str) -> String {
    let mut sanitized = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        "document".to_string()
    } else {
        sanitized
    }
}

fn normalize_pdf_text(raw: &str) -> String {
    raw.replace('\0', "")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn normalize_markdown_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let normalized = raw
        .strip_prefix("\\\\?\\")
        .unwrap_or(raw.as_ref())
        .replace('\\', "/");
    normalized
}

fn markdown_image_reference_path(pdf_path: &Path, image_path: &Path) -> String {
    let base_dir = pdf_path.parent().unwrap_or_else(|| Path::new("."));
    let relative_path = image_path.strip_prefix(base_dir).unwrap_or(image_path);
    normalize_markdown_path(relative_path)
}

fn looks_like_fragmented_segments(segments: &[String]) -> bool {
    if segments.len() < 40 {
        return false;
    }
    let tiny_segments = segments
        .iter()
        .filter(|text| text.chars().count() <= 2)
        .count();
    tiny_segments * 100 / segments.len() >= 80
}
