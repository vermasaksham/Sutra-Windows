//! Writing a note out as a .docx.
//!
//! The frontend hands over a flat document — blocks of runs, with formulas and
//! attachments already rendered to images. That split is deliberate: Rust has
//! neither a TeX engine nor a canvas, and the editor has no business knowing
//! about OOXML. Neither side needs the other's vocabulary.
//!
//! Pictures go in twice where we can. Word since 2016 renders SVG, but the
//! format insists on a raster copy alongside it: the vector is an *extension*
//! hanging off the `<a:blip>` that names the PNG, so a reader that does not
//! know the extension still shows something. So a formula arrives here as both
//! an SVG and a PNG, and both end up in the package. Modern Word prints the
//! vector; LibreOffice and Word 2013 print the raster.

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
        /// A PNG data URL. Always present: Word needs a raster copy even when
        /// a vector one is supplied.
        data: String,
        /// An `image/svg+xml` data URL, when the picture has a vector form.
        #[serde(default)]
        svg: Option<String>,
        width: u32,
        height: u32,
        #[serde(default)]
        alt: String,
    },
}

/// A vector picture waiting to be woven into the finished package.
///
/// `blip` is the relationship id docx-rs minted for the PNG. It is how we find
/// the right `<a:blip>` in `word/document.xml` afterwards — matching on the id
/// rather than on the order pictures happen to appear in.
struct VectorCopy {
    blip: String,
    bytes: Vec<u8>,
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

fn decode_data_url(data: &str) -> Result<Vec<u8>> {
    // Data URLs arrive as `data:<type>;base64,....`; take what follows the
    // comma. The declared type is ignored — the caller knows what it asked for,
    // and for the raster copy `is_png` below checks the bytes themselves.
    let payload = data.split_once(',').map(|(_, rest)| rest).unwrap_or(data);
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| SutraError::Export(format!("could not decode an image: {e}")))
}

/// The eight bytes every PNG starts with.
///
/// Worth checking, because `Pic::new` does not check: handed anything it cannot
/// decode it panics, and this binary is built with `panic = "abort"`, so a
/// stray image would take the whole app down rather than fail one export.
fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
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
    // Filled in as pictures are added; drained after packing.
    let mut vectors: Vec<VectorCopy> = Vec::new();

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
                svg,
                width,
                height,
                alt,
            } => {
                let bytes = decode_data_url(data)?;
                if !is_png(&bytes) {
                    return Err(SutraError::Export(
                        "an image did not arrive as a PNG, so it cannot be embedded".into(),
                    ));
                }
                // A rendered formula carries its own size; an attachment
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
                if let Some(svg) = svg {
                    // `picture.id` is the relationship id docx-rs will write
                    // into the `<a:blip>`. Grab it now, while we still know
                    // which picture it belongs to.
                    vectors.push(VectorCopy {
                        blip: picture.id.clone(),
                        bytes: decode_data_url(svg)?,
                    });
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

    // Packed into memory rather than straight to the file, because the vector
    // copies are added by reading the finished package back and rewriting it.
    let mut packed = std::io::Cursor::new(Vec::new());
    docx.build()
        .pack(&mut packed)
        .map_err(|e| SutraError::Export(e.to_string()))?;
    let mut bytes = packed.into_inner();
    if !vectors.is_empty() {
        bytes = weave_vectors(&bytes, &vectors)?;
    }
    std::fs::write(path, &bytes)?;
    Ok(())
}

/// The GUID that marks the SVG extension on a picture. Fixed by the format —
/// Word looks for exactly this string, it is not a name we get to choose.
const SVG_EXTENSION_URI: &str = "{96DAC541-7B7A-43D3-8B79-37D633B846F1}";
const SVG_NAMESPACE: &str = "http://schemas.microsoft.com/office/drawing/2016/SVG/main";
const IMAGE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

fn relationship_id(n: usize) -> String {
    // docx-rs mints `rId{n}` and `rIdImage{n}`, so this prefix cannot collide.
    format!("rIdSutraSvg{n}")
}

fn media_name(n: usize) -> String {
    format!("sutraSvg{n}.svg")
}

/// Add the vector copies to a finished package.
///
/// Three parts change and N are added. docx-rs cannot do this itself — its
/// zipper hardcodes a `.png` suffix on every media entry — so the package is
/// read back and rewritten. Every other entry is copied through untouched.
fn weave_vectors(package: &[u8], vectors: &[VectorCopy]) -> Result<Vec<u8>> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(package))
        .map_err(|e| SutraError::Export(format!("could not reopen the document: {e}")))?;

    // Patch document.xml first: a vector whose picture cannot be found is
    // dropped, and only the ones that survive get a part and a relationship.
    // Otherwise the package would carry a relationship pointing at nothing.
    let document = read_entry(&mut archive, "word/document.xml")?;
    let (document, attached) = attach_svg_blips(&document, vectors);
    if attached.is_empty() {
        return Ok(package.to_vec());
    }

    let content_types = declare_svg(&read_entry(&mut archive, "[Content_Types].xml")?)
        .ok_or_else(|| SutraError::Export("the document has no content types".into()))?;
    let rels = add_svg_relationships(
        &read_entry(&mut archive, "word/_rels/document.xml.rels")?,
        &attached,
    )
    .ok_or_else(|| SutraError::Export("the document has no relationships".into()))?;

    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let zipped = |e: zip::result::ZipError| SutraError::Export(format!("could not rewrite: {e}"));

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zipped)?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            writer.add_directory(name, stored).map_err(zipped)?;
            continue;
        }
        let replacement = match name.as_str() {
            "word/document.xml" => Some(document.as_bytes()),
            "[Content_Types].xml" => Some(content_types.as_bytes()),
            "word/_rels/document.xml.rels" => Some(rels.as_bytes()),
            _ => None,
        };
        writer.start_file(&name, stored).map_err(zipped)?;
        match replacement {
            Some(bytes) => writer.write_all(bytes)?,
            None => {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                writer.write_all(&bytes)?;
            }
        }
    }

    for &n in &attached {
        writer
            .start_file(format!("word/media/{}", media_name(n)), deflated)
            .map_err(zipped)?;
        writer.write_all(&vectors[n].bytes)?;
    }

    Ok(writer.finish().map_err(zipped)?.into_inner())
}

fn read_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String> {
    use std::io::Read;
    let mut entry = archive
        .by_name(name)
        .map_err(|e| SutraError::Export(format!("{name} is missing from the document: {e}")))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    Ok(text)
}

/// Hang an `<asvg:svgBlip>` off each picture that has a vector copy.
///
/// Returns the patched XML and the indices that were actually attached. A
/// picture whose `<a:blip>` is not where we expect is skipped rather than
/// failing the export — the reader still gets the PNG.
fn attach_svg_blips(xml: &str, vectors: &[VectorCopy]) -> (String, Vec<usize>) {
    let mut out = xml.to_string();
    let mut attached = Vec::new();

    for (n, vector) in vectors.iter().enumerate() {
        let opening = format!("<a:blip r:embed=\"{}\"", vector.blip);
        let Some(start) = out.find(&opening) else {
            continue;
        };
        let Some(offset) = out[start..].find("/>") else {
            continue;
        };
        let end = start + offset;
        // If another tag opened before that `/>`, this is not the self-closing
        // element we assumed, and blindly splicing would corrupt the XML.
        if out[start + 1..end].contains('<') {
            continue;
        }
        // The extension list is the last child of a blip, and the namespace is
        // declared inline so the document root does not have to be touched.
        let extension = format!(
            "><a:extLst><a:ext uri=\"{SVG_EXTENSION_URI}\">\
             <asvg:svgBlip xmlns:asvg=\"{SVG_NAMESPACE}\" r:embed=\"{}\"/>\
             </a:ext></a:extLst></a:blip>",
            relationship_id(n)
        );
        out.replace_range(end..end + 2, &extension);
        attached.push(n);
    }

    (out, attached)
}

fn declare_svg(content_types: &str) -> Option<String> {
    if content_types.contains("Extension=\"svg\"") {
        return Some(content_types.to_owned());
    }
    let at = content_types.rfind("</Types>")?;
    let mut out = content_types.to_owned();
    out.insert_str(
        at,
        "<Default Extension=\"svg\" ContentType=\"image/svg+xml\"/>",
    );
    Some(out)
}

fn add_svg_relationships(rels: &str, attached: &[usize]) -> Option<String> {
    let at = rels.rfind("</Relationships>")?;
    let mut additions = String::new();
    for &n in attached {
        additions.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{IMAGE_RELATIONSHIP}\" Target=\"media/{}\"/>",
            relationship_id(n),
            media_name(n)
        ));
    }
    let mut out = rels.to_owned();
    out.insert_str(at, &additions);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A 1x1 PNG, so image handling is exercised with real bytes.
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    /// A black square, so the vector path is exercised with real markup.
    const SVG: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxMCIgaGVpZ2h0PSIxMCI+PHBhdGggZD0iTTAgMGgxMHYxMEgweiIvPjwvc3ZnPg==";

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
                    svg: Some(SVG.into()),
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
            svg: None,
            width: 10,
            height: 10,
            alt: String::new(),
        }];
        let path = std::env::temp_dir().join("sutra-bad.docx");
        assert!(write_docx(&document, &path).is_err());
        let _ = std::fs::remove_file(path);
    }
    /// Read one entry out of a written .docx.
    fn entry(path: &std::path::Path, name: &str) -> String {
        let file = std::fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut text = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    #[test]
    fn a_vector_copy_is_woven_in_beside_the_raster_one() {
        let path = write_to_temp(&sample());

        // The SVG is a part of its own...
        let svg = entry(&path, "word/media/sutraSvg0.svg");
        assert!(svg.contains("<path"), "the SVG part is not the SVG we sent");

        // ...declared in the content types...
        assert!(entry(&path, "[Content_Types].xml").contains("image/svg+xml"));

        // ...reachable by a relationship...
        let rels = entry(&path, "word/_rels/document.xml.rels");
        assert!(
            rels.contains(r#"Id="rIdSutraSvg0""#),
            "no relationship: {rels}"
        );
        assert!(rels.contains("Target=\"media/sutraSvg0.svg\""));

        // ...and hung off the picture that already carries the PNG.
        let document = entry(&path, "word/document.xml");
        assert!(
            document.contains("asvg:svgBlip"),
            "the blip was not extended"
        );
        assert!(document.contains(SVG_EXTENSION_URI));
        // The raster copy has to survive: it is what old readers show.
        assert!(document.contains("<a:blip r:embed=\"rIdImage"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_document_without_vectors_is_left_exactly_as_packed() {
        // Nothing to weave means nothing to rewrite, so the package should be
        // byte-for-byte what docx-rs produced.
        let mut document = sample();
        for block in &mut document.blocks {
            if let Block::Image { svg, .. } = block {
                *svg = None;
            }
        }
        let path = write_to_temp(&document);
        let bytes = std::fs::read(&path).unwrap();
        assert!(zip::ZipArchive::new(std::io::Cursor::new(&bytes)).is_ok());
        assert!(!entry(&path, "word/document.xml").contains("asvg"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_blip_that_cannot_be_found_is_skipped_not_spliced() {
        let vectors = vec![VectorCopy {
            blip: "rIdImageNoSuchThing".into(),
            bytes: b"<svg/>".to_vec(),
        }];
        let (out, attached) = attach_svg_blips("<w:document/>", &vectors);
        assert!(attached.is_empty());
        assert_eq!(out, "<w:document/>");
    }

    #[test]
    fn a_blip_with_children_is_left_alone() {
        // Only the self-closing form is safe to splice. Anything else and we
        // would be guessing where the element ends.
        let vectors = vec![VectorCopy {
            blip: "rIdImage1".into(),
            bytes: b"<svg/>".to_vec(),
        }];
        let xml = r#"<a:blip r:embed="rIdImage1"><a:alphaModFix/></a:blip>"#;
        let (out, attached) = attach_svg_blips(xml, &vectors);
        assert!(attached.is_empty());
        assert_eq!(out, xml);
    }

    #[test]
    fn a_non_png_image_is_refused_rather_than_panicking() {
        // `Pic::new` panics on bytes it cannot decode, and this binary aborts
        // on panic, so the guard in front of it is load-bearing.
        let mut document = sample();
        document.blocks = vec![Block::Image {
            data: SVG.into(),
            svg: None,
            width: 10,
            height: 10,
            alt: String::new(),
        }];
        let path = std::env::temp_dir().join("sutra-not-png.docx");
        assert!(write_docx(&document, &path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
