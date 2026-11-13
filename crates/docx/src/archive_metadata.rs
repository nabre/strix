//! Bounded archive indexing: only the two property parts are inflated. The body,
//! pictures, relationships and fonts are deliberately never opened.
use crate::{DocxError, xml};
use std::io::{Read, Seek};
use wordcraft_doc::Document;

const MAX_PART: u64 = 256 * 1024;

/// Reads archive properties from a seekable OOXML package without parsing its body.
pub fn read_archive_metadata<R: Read + Seek>(reader: R) -> Result<Document, DocxError> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| DocxError::Zip(e.to_string()))?;
    let mut doc = Document::new();
    for name in ["docProps/core.xml", "docProps/custom.xml"] {
        let part = match zip.by_name(name) {
            Ok(part) => part,
            Err(zip::result::ZipError::FileNotFound) => continue,
            Err(e) => return Err(DocxError::Zip(e.to_string())),
        };
        if part.size() > MAX_PART {
            return Err(DocxError::Limit(format!("{name} exceeds metadata limit")));
        }
        let mut data = Vec::new();
        part.take(MAX_PART + 1).read_to_end(&mut data).map_err(|e| DocxError::Zip(e.to_string()))?;
        if data.len() as u64 > MAX_PART {
            return Err(DocxError::Limit(name.into()));
        }
        let root = xml::parse(&data)?;
        if name.ends_with("custom.xml") {
            doc.custom_props = crate::custom::read(&root);
        } else {
            for el in root.els() {
                let text = el.text().trim().to_string();
                match el.name.as_str() {
                    "dc:title" => doc.core.title = text,
                    "dc:subject" => doc.core.subject = text,
                    "dc:creator" => doc.core.creator = text,
                    "cp:keywords" => doc.core.keywords = text,
                    "dc:description" => doc.core.description = text,
                    _ => {}
                }
            }
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    #[test]
    fn indexes_properties_even_when_the_body_is_invalid() {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        z.start_file("docProps/core.xml", options).unwrap();
        z.write_all(br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:subject>Fisica &amp; chimica</dc:subject><cp:keywords>laboratorio</cp:keywords></cp:coreProperties>"#).unwrap();
        z.start_file("word/document.xml", options).unwrap();
        z.write_all(b"not XML").unwrap();
        let doc = read_archive_metadata(Cursor::new(z.finish().unwrap().into_inner())).unwrap();
        assert_eq!(doc.core.subject, "Fisica & chimica");
        assert_eq!(doc.core.keywords, "laboratorio");
    }
}
