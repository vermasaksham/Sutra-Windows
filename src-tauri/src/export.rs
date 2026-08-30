//! Writing a note out as a .docx.
//!
//! The frontend hands over a flat document — blocks of runs, with formulas and
//! attachments already rasterised to PNG. That split is deliberate: Rust has
//! neither a TeX engine nor a canvas, and the editor has no business knowing
//! about OOXML. Neither side needs the other's vocabulary.

use crate::error::{Result, SutraError};
use base64::Engine;
use docx_rs::*;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Run {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub strike: bool,
    #[serde(default)]
    pub link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    Heading {
        level: usize,
        runs: Vec<Run>,
    },
    Paragraph {
        runs: Vec<Run>,
    },
    Quote {
        runs: Vec<Run>,
    },
    Code {
        text: String,
    },
    ListItem {
        ordered: bool,
        depth: usize,
        #[serde(default)]
        checked: Option<bool>,
        runs: Vec<Run>,
    },
    Divider,
    Table {
        rows: Vec<Vec<String>>,
        header_row: bool,
    },
    Image {
        data: String,
        width: u32,
        height: u32,
        #[serde(default)]
        alt: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct ExportDocument {
    pub title: String,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub references: Vec<String>,
}

/// Word measures pictures in EMU: 914400 to the inch, and a screen pixel is
/// conventionally 1/96 inch.
const EMU_PER_PIXEL: u32 = 914_400 / 96;

/// Keep a picture inside the text column of a portrait A4 page with the default
/// margins, which is about 6.3 inches.
const MAX_WIDTH_PX: u32 = 600;

fn decode_png(data: &str) -> Result<Vec<u8>> {
    // Data URLs arrive as `data:image/png;base64,....`; take what follows the
    // comma and ignore the declared type, since we only ever send PNG.
    let payload = data.split_once(',').map(|(_, rest)| rest).unwrap_or(data);
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| SutraError::Export(format!("could not decode an image: {e}")))
}

fn styled(run: &Run) -> docx_rs::Run {
    let mut out = docx_rs::Run::new().add_text(&run.text);
    if run.bold {
        out = out.bold();
    }
    if run.italic {
        out = out.italic();
    }
    if run.strike {
        out = out.strike();
    }
    if run.code {
        // Word has no "inline code" character style by default, so say what we
        // mean directly rather than relying on one existing in the template.
        out = out.fonts(RunFonts::new().ascii("Consolas")).size(20);
    }
    out
}

fn paragraph_of(runs: &[Run]) -> Paragraph {
    let mut paragraph = Paragraph::new();
    for run in runs {
        paragraph = paragraph.add_run(styled(run));
        // A link's destination is lost otherwise. Rather than build a
        // relationship for every link, the URL follows the text — clumsy, but
        // it survives, and a reader can follow it.
        if let Some(url) = &run.link {
            if !url.is_empty() && url != &run.text {
                paragraph =
                    paragraph.add_run(docx_rs::Run::new().add_text(format!(" <{url}>")).italic());
            }
        }
    }
    paragraph
}

/// Build the .docx and write it to `path`.
pub fn write_docx(document: &ExportDocument, path: &Path) -> Result<()> {
    let mut docx = Docx::new().add_paragraph(
        Paragraph::new()
            .add_run(
                docx_rs::Run::new()
                    .add_text(&document.title)
                    .bold()
                    .size(36),
            )
            .style("Title"),
    );

    for block in &document.blocks {
        docx = match block {
            Block::Heading { level, runs } => docx.add_paragraph(
                paragraph_of(runs).style(&format!("Heading{}", level.clamp(&1, &6))),
            ),
            Block::Paragraph { runs } => docx.add_paragraph(paragraph_of(runs)),
            Block::Quote { runs } => docx.add_paragraph(paragraph_of(runs).style("Quote").indent(
                Some(720),
                None,
                None,
                None,
            )),
            Block::Code { text } => {
                // Each line its own paragraph: Word does not honour newlines
                // inside a run, so a single paragraph would collapse the
                // listing onto one line.
                let mut out = docx;
                for line in text.lines() {
                    out = out.add_paragraph(
                        Paragraph::new()
                            .add_run(
                                docx_rs::Run::new()
                                    .add_text(line)
                                    .fonts(RunFonts::new().ascii("Consolas"))
                                    .size(20),
                            )
                            .indent(Some(360), None, None, None),
                    );
                }
                out
            }
            Block::ListItem {
                ordered,
                depth,
                checked,
                runs,
            } => {
                let mut paragraph = paragraph_of(runs);
                // A checkbox has no Word equivalent that survives round-tripping,
                // so it becomes a character that reads the same on paper.
                if let Some(done) = checked {
                    let mark = if *done { "\u{2611} " } else { "\u{2610} " };
                    let mut rebuilt = Paragraph::new().add_run(docx_rs::Run::new().add_text(mark));
                    for run in runs {
                        rebuilt = rebuilt.add_run(styled(run));
                    }
                    paragraph = rebuilt;
                }
                docx.add_paragraph(paragraph.numbering(
                    NumberingId::new(if *ordered { 2 } else { 1 }),
                    IndentLevel::new(*depth),
                ))
            }
            Block::Divider => docx.add_paragraph(
                Paragraph::new().add_run(docx_rs::Run::new().add_text("―".repeat(30))),
            ),
            Block::Table { rows, header_row } => {
                let table_rows: Vec<TableRow> = rows
                    .iter()
                    .enumerate()
                    .map(|(index, cells)| {
                        TableRow::new(
                            cells
                                .iter()
                                .map(|cell| {
                                    let bold = *header_row && index == 0;
                                    let mut run = docx_rs::Run::new().add_text(cell);
                                    if bold {
                                        run = run.bold();
                                    }
                                    TableCell::new().add_paragraph(Paragraph::new().add_run(run))
                                })
                                .collect(),
                        )
                    })
                    .collect();
                docx.add_table(Table::new(table_rows).set_grid(vec![]))
            }
            Block::Image {
                data,
                width,
                height,
                alt,
            } => {
                let bytes = decode_png(data)?;
                // A rasterised formula carries its own size; an attachment
                // arrives as 0 and is scaled to fit the column.
                let (w, h) = if *width == 0 || *height == 0 {
                    (MAX_WIDTH_PX, 0)
                } else if *width > MAX_WIDTH_PX {
                    (MAX_WIDTH_PX, height * MAX_WIDTH_PX / width.max(&1))
                } else {
                    (*width, *height)
                };
                let mut picture = Pic::new(&bytes);
                if h > 0 {
                    picture = picture.size(w * EMU_PER_PIXEL, h * EMU_PER_PIXEL);
                }
                let mut out = docx.add_paragraph(
                    Paragraph::new().add_run(docx_rs::Run::new().add_image(picture)),
                );
                // The LaTeX source, or the alt text, kept as a caption. An
                // equation that is only a picture is unsearchable otherwise.
                if !alt.is_empty() {
                    out = out.add_paragraph(
                        Paragraph::new()
                            .add_run(docx_rs::Run::new().add_text(alt).italic().size(16)),
                    );
                }
                out
            }
        };
    }

    if !document.references.is_empty() {
        docx = docx.add_paragraph(
            Paragraph::new()
                .add_run(docx_rs::Run::new().add_text("References").bold().size(28))
                .style("Heading2"),
        );
        for reference in &document.references {
            docx = docx
                .add_paragraph(Paragraph::new().add_run(docx_rs::Run::new().add_text(reference)));
        }
    }

    let file = std::fs::File::create(path)?;
    docx.build()
        .pack(file)
        .map_err(|e| SutraError::Export(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A 1x1 PNG, so image handling is exercised with real bytes.
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    fn sample() -> ExportDocument {
        ExportDocument {
            title: "Sb2Se3 growth log".into(),
            blocks: vec![
                Block::Heading {
                    level: 2,
                    runs: vec![Run {
                        text: "Transport reaction".into(),
                        bold: false,
                        italic: false,
                        code: false,
                        strike: false,
                        link: None,
                    }],
                },
                Block::Paragraph {
                    runs: vec![
                        Run {
                            text: "Ribbons align ".into(),
                            bold: false,
                            italic: false,
                            code: false,
                            strike: false,
                            link: None,
                        },
                        Run {
                            text: "strongly".into(),
                            bold: true,
                            italic: false,
                            code: false,
                            strike: false,
                            link: None,
                        },
                    ],
                },
                Block::Quote {
                    runs: vec![Run {
                        text: "Worth isolating.".into(),
                        bold: false,
                        italic: false,
                        code: false,
                        strike: false,
                        link: None,
                    }],
                },
                Block::Code {
                    text: "hkl 2theta\n120 28.2".into(),
                },
                Block::ListItem {
                    ordered: false,
                    depth: 0,
                    checked: Some(true),
                    runs: vec![Run {
                        text: "XRD done".into(),
                        bold: false,
                        italic: false,
                        code: false,
                        strike: false,
                        link: None,
                    }],
                },
                Block::Divider,
                Block::Table {
                    rows: vec![
                        vec!["Parameter".into(), "Value".into()],
                        vec!["Source".into(), "560 C".into()],
                    ],
                    header_row: true,
                },
                Block::Image {
                    data: PNG.into(),
                    width: 320,
                    height: 180,
                    alt: "\\ce{Sb2Se3}".into(),
                },
            ],
            references: vec!["Zhou et al. (2019) Quasi-1D Sb2Se3 ribbons doi:10.1000/xyz".into()],
        }
    }

    fn write_to_temp(document: &ExportDocument) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("sutra-export-{}.docx", ulid::Ulid::generate()));
        write_docx(document, &path).unwrap();
        path
    }

    #[test]
    fn a_docx_is_a_valid_zip_with_the_expected_parts() {
        // A .docx is an OPC package. If these parts are missing or the zip is
        // malformed, Word refuses the file outright — so this is the check that
        // matters most, and it cannot be made by eye.
        let path = write_to_temp(&sample());
        let file = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).expect("not a valid zip");

        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();

        for required in ["[Content_Types].xml", "word/document.xml", "_rels/.rels"] {
            assert!(
                names.iter().any(|n| n == required),
                "missing {required} in {names:?}"
            );
        }

        let mut xml = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.starts_with("<?xml"), "document.xml is not XML");
        assert!(xml.contains("Sb2Se3 growth log"), "title missing");
        assert!(xml.contains("Transport reaction"), "heading missing");
        assert!(xml.contains("Worth isolating."), "quote missing");
        assert!(xml.contains("Zhou et al."), "bibliography missing");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_code_block_keeps_its_lines_apart() {
        // Word ignores newlines inside a run, so a listing written as one
        // paragraph collapses onto a single line.
        let path = write_to_temp(&sample());
        let file = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut xml = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();

        let before = xml.find("hkl 2theta").expect("first line missing");
        let after = xml.find("120 28.2").expect("second line missing");
        assert!(
            xml[before..after].contains("</w:p>"),
            "the two code lines share a paragraph"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_image_is_embedded_as_a_media_part() {
        let path = write_to_temp(&sample());
        let file = std::fs::File::open(&path).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| {
                let mut z = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
                z.by_index(i).unwrap().name().to_string()
            })
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("word/media/")),
            "no embedded image in {names:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    /// Leaves a file behind on purpose, so tools other than our own can judge
    /// it. Run with `cargo test --bins -- --ignored emit_for_inspection`.
    #[test]
    #[ignore]
    fn emit_for_inspection() {
        write_docx(&sample(), std::path::Path::new("/tmp/sutra-check.docx")).unwrap();
    }

    #[test]
    fn a_malformed_image_is_an_error_not_a_panic() {
        let mut document = sample();
        document.blocks = vec![Block::Image {
            data: "data:image/png;base64,not-base64!!".into(),
            width: 10,
            height: 10,
            alt: String::new(),
        }];
        let path = std::env::temp_dir().join("sutra-bad.docx");
        assert!(write_docx(&document, &path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
