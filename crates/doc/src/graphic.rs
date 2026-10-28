// Modified by the Strix contributors (fork of WordCraft); see NOTICE.
//! Charts, SmartArt diagrams and native ink: drawings kept as neutral items ready to draw. Charts and SmartArt
//! Strix can edit also keep their model ([`crate::chart::ChartSpec`],
//! [`crate::smart_art::SmartArtSpec`]); the rest are shown, not edited.

use serde::{Deserialize, Serialize};

use crate::para::ShapeKind;
use crate::props::Rgb;

/// A chart, SmartArt diagram or native ink preview, as items ready to draw (and, for an editable chart or SmartArt
/// graphic, its model).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graphic {
    pub kind: GraphicKind,
    /// What to draw, in points from the object's top-left corner.
    #[serde(default)]
    pub items: Vec<GraphicItem>,
    /// The size (points) the items were built for. Drawn scaled to the object's rectangle when
    /// that differs (layout shrinks oversized inline objects); 0 means the rectangle's own size.
    #[serde(default)]
    pub w: f32,
    #[serde(default)]
    pub h: f32,
    /// Where the graphic was read from, so saving writes the original drawing itself back (not
    /// only these items). `None` for graphics made some other way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<std::sync::Arc<Embedded>>,
    /// A SmartArt graphic Strix can edit (one it made, or one read back exactly as it wrote
    /// it): saving writes the diagram parts from it, and the items are drawn from it. See
    /// [`crate::smart_art`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart_art: Option<std::sync::Arc<crate::smart_art::SmartArtSpec>>,
    /// A chart Strix can edit (one it made, or one read back that holds nothing more): saving
    /// writes the chart part from it, and the items are drawn from it. See [`crate::chart`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chart: Option<std::sync::Arc<crate::chart::ChartSpec>>,
    /// A CAD drawing's source (the `strix-cad` JSON); the items are rendered from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cad: Option<std::sync::Arc<str>>,
    /// A CAD drawing's picture as SVG (what the items show), saved with the source so other
    /// programs show the drawing too. Rebuilt with the items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg: Option<std::sync::Arc<str>>,
}

/// What an object read from a file needs to be written back as it was: its own markup in the
/// source format and the package parts that markup refers to. Never edited; Strix only moves
/// and resizes the object around it. The parts themselves (bytes, content type and their own
/// relationships) are kept in [`crate::Document::passthrough`] under their part names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Embedded {
    /// The object's markup as a standalone XML element with its namespace declarations (DOCX:
    /// the `a:graphic` of a chart, SmartArt diagram or native ink, an ink `mc:AlternateContent`,
    /// the `w:object` of an OLE object, or the `w:pict` of a signature line).
    pub xml: String,
    /// The relationships it uses, by the ids in `xml` (or in its parts, for a SmartArt drawing).
    pub refs: Vec<EmbeddedRef>,
}

/// One relationship an [`Embedded`] object uses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedRef {
    /// The relationship id the markup uses.
    pub id: String,
    /// Relationship type (a URI).
    pub kind: String,
    /// The part name (a `Document::passthrough` key), or the URL of an external target.
    pub target: String,
    pub external: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicKind {
    #[default]
    Chart,
    Diagram,
    /// Word's native InkML, kept opaque; displayed as its fallback picture when available.
    Ink,
    /// A Strix CAD drawing (docs/cad.md): [`Graphic::cad`] holds its source, the items are drawn
    /// from it by the engine.
    Cad,
}

impl GraphicKind {
    /// What the graphic is, in words: the alt text of one that has none of its own.
    pub fn noun(self) -> &'static str {
        match self {
            GraphicKind::Chart => "chart",
            GraphicKind::Diagram => "diagram",
            GraphicKind::Ink => "ink",
            GraphicKind::Cad => "drawing",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphicItem {
    /// A preset shape filling `rect` ([x, y, w, h]).
    Shape {
        rect: [f32; 4],
        kind: ShapeKind,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        #[serde(default)]
        stroke_width: f32,
    },
    /// A path: polygons, pie slices, line series.
    Path {
        segs: Vec<PathSeg>,
        fill: Option<Rgb>,
        stroke: Option<Rgb>,
        #[serde(default)]
        stroke_width: f32,
    },
    /// One line of text in `rect`, aligned horizontally by `align`, centred vertically; turned
    /// `angle` degrees counter-clockwise about `rect`'s centre (a CAD drawing's rotated text).
    Text {
        rect: [f32; 4],
        text: String,
        size: f32,
        color: Rgb,
        #[serde(default)]
        bold: bool,
        /// Italic (a drawing's parameter symbols, set as the equations set them).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        italic: bool,
        #[serde(default)]
        align: TextAlign,
        #[serde(default)]
        font: Option<String>,
        #[serde(default, skip_serializing_if = "is_level")]
        angle: f32,
    },
    /// A picture (`Document::media` key) stretched into `rect`.
    Image { rect: [f32; 4], media: String },
}

/// A text's angle that leaves it level (not written).
fn is_level(angle: &f32) -> bool {
    *angle == 0.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathSeg {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}
