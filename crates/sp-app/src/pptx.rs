//! `.pptx` files read directly into editable slides: no program is started. A `.pptx` is a zip
//! of XML parts; each slide's text boxes, pictures, backgrounds and simple shapes become
//! objects on a slide, placed and sized by scaling the deck's slide to SoftPresenter's, with
//! what a placeholder leaves unsaid taken from its slide layout and master, and colours and
//! fonts from the theme.
//!
//! The file is not trusted. The zip is read through [`sp_model::bounded_zip`] (bounded sizes
//! and entry counts, overlapping entries refused, a name used only as a key), XML parts are
//! refused when they declare a document type (so no entity is expanded and nothing is fetched),
//! nest too deeply or carry too many attributes, and a picture over [`LARGEST_PICTURE`] is
//! left out.
//!
//! A slide holds one look per text box: a box that mixes sizes, colours or fonts is given the
//! look most of its letters have, and its slide is marked to check. What can't be mapped (charts,
//! SmartArt, tables, 3D, embedded video and OLE objects) is drawn from the picture the file
//! carries for it, when it carries one (as a fallback beside the object, or a video's poster),
//! and otherwise left out; either way the slide is marked to check.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use roxmltree::{Document, Node};
use sp_model::bounded_zip::{ZipLimits, ZipRefusal};
use sp_model::{
    Align, Caps, Gradient, GradientKind, ImageFit, ListStyle, Object, ObjectKind, Rgba, Scaling, Shape, Slide,
    SlideRect, StrokeLook, TextStyle, VAlign,
};

/// The largest `.pptx` read.
pub const LARGEST_FILE: u64 = 512 * 1024 * 1024;
/// The largest picture taken from a deck; a larger one is left out.
pub const LARGEST_PICTURE: usize = 32 * 1024 * 1024;
/// The largest XML part read.
const LARGEST_PART: usize = 16 * 1024 * 1024;
/// How deep an XML part may nest, and how many attributes one element may carry.
const MOST_DEPTH: usize = 64;
const MOST_ATTRIBUTES: usize = 64;
/// Shapes inside shapes (groups) are followed this deep.
const MOST_GROUP_DEPTH: usize = 8;

const LIMITS: ZipLimits =
    ZipLimits { file: LARGEST_FILE, entry: 64 * 1024 * 1024, unpacked: 512 * 1024 * 1024, entries: 20_000 };

const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const EMU_PER_POINT: f64 = 12_700.0;
/// The distance from one line to the next, as a multiple of the font size, at "single" spacing.
const SINGLE: f64 = 1.2;

/// What reading a deck gave.
#[derive(Debug, Default)]
pub struct Read {
    /// The slides, in order.
    pub slides: Vec<Slide>,
    /// The slides to check, by number (from 1), and why.
    pub check: Vec<(usize, Vec<String>)>,
}

/// The parts of a package that are read.
struct Package {
    xml: HashMap<String, String>,
    media: HashMap<String, Vec<u8>>,
}

fn wanted(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("ppt/") else { return false };
    if rest.starts_with("embeddings/") || rest.starts_with("activeX/") {
        return false;
    }
    let ending = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ending.as_str(), "xml" | "rels" | "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff")
}

fn refusal(name: &str, why: ZipRefusal) -> String {
    match why {
        ZipRefusal::Damaged => format!("{name} is a damaged file."),
        ZipRefusal::Overlap => format!("{name} is a damaged file: its parts overlap."),
        ZipRefusal::TooMany => format!("{name} has too many parts in it to be a presentation."),
        ZipRefusal::UnpacksTooMuch => format!("{name} unpacks to more than any presentation, so it wasn't read."),
    }
}

fn open_package(bytes: &[u8], name: &str) -> Result<Package, String> {
    if bytes.len() as u64 > LIMITS.file {
        return Err(format!("{name} is larger than any presentation, so it wasn't read."));
    }
    if !sp_model::bounded_zip::is_zip(bytes) {
        return Err(format!("{name} isn't a .pptx file (it isn't a zip)."));
    }
    let mut package = Package { xml: HashMap::new(), media: HashMap::new() };
    let mut too_big = false;
    sp_model::bounded_zip::for_each_entry(bytes, &LIMITS, wanted, |entry, read, _| {
        let lower = entry.to_ascii_lowercase();
        if lower.ends_with(".xml") || lower.ends_with(".rels") {
            if read.len() > LARGEST_PART {
                too_big = true;
                return Ok(());
            }
            package.xml.insert(entry, String::from_utf8_lossy(&read).into_owned());
        } else {
            // A picture over the limit is kept as nothing, so the slide using it is named.
            package.media.insert(entry, if read.len() > LARGEST_PICTURE { Vec::new() } else { read });
        }
        Ok(())
    })
    .map_err(|why| refusal(name, why))?;
    if too_big {
        return Err(format!("{name} has a part larger than any presentation's, so it wasn't read."));
    }
    if !package.xml.contains_key("ppt/presentation.xml") {
        return Err(format!("{name} isn't a .pptx file (it has no presentation in it)."));
    }
    Ok(package)
}

// ---- XML helpers -------------------------------------------------------------------------

type Doc<'p> = Rc<Document<'p>>;

fn is(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn child<'a, 'b>(node: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    node.children().find(|c| is(*c, name))
}

fn children<'a, 'b>(node: Node<'a, 'b>, name: &'static str) -> impl Iterator<Item = Node<'a, 'b>> {
    node.children().filter(move |c| is(*c, name))
}

fn num(node: Node, name: &str) -> Option<f64> {
    node.attribute(name)?.trim().parse().ok()
}

fn flag(node: Node, name: &str) -> Option<bool> {
    node.attribute(name).map(|v| matches!(v, "1" | "true"))
}

fn r_attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name && a.namespace() == Some(R_NS)).map(|a| a.value())
}

/// Refuses documents that nest too deeply or carry too many attributes on an element.
fn checked(document: &Document) -> bool {
    let mut depth_of: HashMap<roxmltree::NodeId, usize> = HashMap::new();
    for node in document.descendants() {
        let depth = node.parent().and_then(|p| depth_of.get(&p.id()).copied()).map_or(0, |d| d + 1);
        if depth > MOST_DEPTH || (node.is_element() && node.attributes().len() > MOST_ATTRIBUTES) {
            return false;
        }
        depth_of.insert(node.id(), depth);
    }
    true
}

fn parse(text: &str) -> Option<Document<'_>> {
    let options =
        roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 1_000_000, ..roxmltree::ParsingOptions::default() };
    let document = Document::parse_with_options(text, options).ok()?;
    checked(&document).then_some(document)
}

/// A part's folder.
fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The part a relationship's target names, from the folder of the part that has it, as a name
/// in the package (never a place on disk). `None` for a target that climbs out of the package.
fn resolve(folder: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> =
        if target.starts_with('/') { Vec::new() } else { folder.split('/').filter(|p| !p.is_empty()).collect() };
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// A part's relationships: id to (kind's last word, target part).
type Rels = HashMap<String, (String, String)>;

// ---- colour ------------------------------------------------------------------------------

#[derive(Default)]
struct Theme {
    colors: HashMap<String, Rgba>,
    major: String,
    minor: String,
    /// The part the theme was read from, to look its background fill styles up again.
    path: String,
}

struct Colours<'a> {
    theme: &'a Theme,
    map: &'a HashMap<String, String>,
    /// What `phClr` stands for, where a style gives a colour.
    placeholder: Option<Rgba>,
}

fn hex(value: &str) -> Option<Rgba> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(value, 16).ok()?;
    Some(Rgba::rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn preset_colour(name: &str) -> Option<Rgba> {
    Some(match name {
        "black" => Rgba::BLACK,
        "white" => Rgba::WHITE,
        "red" => Rgba::rgb(255, 0, 0),
        "green" => Rgba::rgb(0, 128, 0),
        "blue" => Rgba::rgb(0, 0, 255),
        "yellow" => Rgba::rgb(255, 255, 0),
        "gray" | "grey" => Rgba::rgb(128, 128, 128),
        "orange" => Rgba::rgb(255, 165, 0),
        "purple" => Rgba::rgb(128, 0, 128),
        _ => return None,
    })
}

fn to_hsl(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = (f64::from(c.r) / 255.0, f64::from(c.g) / 255.0, f64::from(c.b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f64::EPSILON {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64, a: u8) -> Rgba {
    let hue = |p: f64, q: f64, mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (hue(p, q, h + 1.0 / 3.0), hue(p, q, h), hue(p, q, h - 1.0 / 3.0))
    };
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgba { r: byte(r), g: byte(g), b: byte(b), a }
}

impl Colours<'_> {
    /// The colour a colour element (`srgbClr`, `schemeClr`, `sysClr`, `prstClr`) names, with its
    /// changes (luminance, tint, shade, transparency) applied.
    fn of(&self, element: Node) -> Option<Rgba> {
        let value = element.attribute("val").unwrap_or("");
        let mut colour = match element.tag_name().name() {
            "srgbClr" => hex(value)?,
            "sysClr" => element.attribute("lastClr").and_then(hex).or(match value {
                "window" => Some(Rgba::WHITE),
                _ => Some(Rgba::BLACK),
            })?,
            "prstClr" => preset_colour(value)?,
            "schemeClr" => {
                if value == "phClr" {
                    self.placeholder?
                } else {
                    let name = self.map.get(value).map_or(value, String::as_str);
                    *self.theme.colors.get(name)?
                }
            }
            _ => return None,
        };
        for change in element.children().filter(Node::is_element) {
            let v = num(change, "val").unwrap_or(100_000.0) / 100_000.0;
            match change.tag_name().name() {
                "lumMod" | "lumOff" => {
                    let (h, s, l) = to_hsl(colour);
                    let l = if change.tag_name().name() == "lumMod" { l * v } else { l + v };
                    colour = from_hsl(h, s, l.clamp(0.0, 1.0), colour.a);
                }
                "alpha" => colour.a = (v.clamp(0.0, 1.0) * 255.0).round() as u8,
                "tint" => {
                    let mix = |c: u8| (f64::from(c) * v + 255.0 * (1.0 - v)).round() as u8;
                    colour = Rgba { r: mix(colour.r), g: mix(colour.g), b: mix(colour.b), a: colour.a };
                }
                "shade" => {
                    let mix = |c: u8| (f64::from(c) * v).round() as u8;
                    colour = Rgba { r: mix(colour.r), g: mix(colour.g), b: mix(colour.b), a: colour.a };
                }
                _ => {}
            }
        }
        Some(colour)
    }

    /// The colour inside a node that holds one (`solidFill`, `fontRef`, `lnRef`…).
    fn inside(&self, holder: Node) -> Option<Rgba> {
        holder.children().filter(Node::is_element).find_map(|c| self.of(c))
    }
}

fn read_theme(document: &Document) -> Theme {
    let mut theme = Theme::default();
    for node in document.descendants() {
        if is(node, "clrScheme") {
            for entry in node.children().filter(Node::is_element) {
                let name = entry.tag_name().name().to_owned();
                let none = Theme::default();
                let colours = Colours { theme: &none, map: &HashMap::new(), placeholder: None };
                if let Some(colour) = colours.inside(entry) {
                    theme.colors.insert(name, colour);
                }
            }
        } else if is(node, "majorFont") {
            theme.major = child(node, "latin").and_then(|l| l.attribute("typeface")).unwrap_or("").to_owned();
        } else if is(node, "minorFont") {
            theme.minor = child(node, "latin").and_then(|l| l.attribute("typeface")).unwrap_or("").to_owned();
        }
    }
    theme
}

// ---- fills -------------------------------------------------------------------------------

enum Fill {
    None,
    Solid(Rgba),
    Gradient(Rgba, Gradient),
    /// A picture, by its relationship id in the part the fill is in.
    Picture(String),
}

fn read_fill(holder: Node, colours: &Colours) -> Option<Fill> {
    holder.children().filter(Node::is_element).find_map(|element| read_fill_element(element, colours))
}

/// The fill one element is (`noFill`, `solidFill`, `gradFill`, `blipFill`, `pattFill`), if it is one.
fn read_fill_element(element: Node, colours: &Colours) -> Option<Fill> {
    match element.tag_name().name() {
        "noFill" => Some(Fill::None),
        "solidFill" => Some(colours.inside(element).map_or(Fill::None, Fill::Solid)),
        "pattFill" => Some(child(element, "fgClr").and_then(|c| colours.inside(c)).map_or(Fill::None, Fill::Solid)),
        "gradFill" => {
            let mut stops: Vec<(f64, Rgba)> = child(element, "gsLst")
                .map(|list| children(list, "gs").filter_map(|g| Some((num(g, "pos")?, colours.inside(g)?))).collect())
                .unwrap_or_default();
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (Some(first), Some(last)) = (stops.first().copied(), stops.last().copied()) else {
                return Some(Fill::None);
            };
            let angle = child(element, "lin").and_then(|l| num(l, "ang")).unwrap_or(5_400_000.0) / 60_000.0;
            let kind = if child(element, "path").is_some() { GradientKind::Radial } else { GradientKind::Linear };
            // Both ends one colour: no gradient.
            if first.1 == last.1 {
                return Some(Fill::Solid(first.1));
            }
            Some(Fill::Gradient(first.1, Gradient { kind, to: last.1, angle: angle as f32 }))
        }
        "blipFill" => child(element, "blip").and_then(|b| r_attr(b, "embed")).map(|id| Fill::Picture(id.to_owned())),
        _ => None,
    }
}

// ---- text styles -------------------------------------------------------------------------

#[derive(Clone, Default)]
struct Level {
    align: Option<Align>,
    /// Line spacing: a multiple of the font size when `.0` is true, else points.
    line: Option<(bool, f64)>,
    before: Option<(bool, f64)>,
    after: Option<(bool, f64)>,
    bullet: Option<Bullet>,
    size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    colour: Option<Rgba>,
    font: Option<String>,
    caps: Option<Caps>,
}

#[derive(Clone, Copy, PartialEq)]
enum Bullet {
    None,
    Dots,
    Numbers,
}

fn spacing(node: Node) -> Option<(bool, f64)> {
    let inner = node.children().find(Node::is_element)?;
    let v = num(inner, "val")?;
    match inner.tag_name().name() {
        "spcPct" => Some((true, v / 100_000.0)),
        "spcPts" => Some((false, v / 100.0)),
        _ => None,
    }
}

impl Level {
    /// Puts on top what a paragraph-properties element (`pPr`, `lvl1pPr`) says, and the run
    /// settings in its `defRPr`.
    fn overlay_paragraph(&mut self, node: Node, colours: &Colours, theme: &Theme) {
        if let Some(a) = node.attribute("algn") {
            self.align = match a {
                "l" => Some(Align::Left),
                "ctr" => Some(Align::Center),
                "r" => Some(Align::Right),
                "just" | "dist" | "justLow" => Some(Align::Justify),
                _ => self.align,
            };
        }
        for element in node.children().filter(Node::is_element) {
            match element.tag_name().name() {
                "lnSpc" => self.line = spacing(element).or(self.line),
                "spcBef" => self.before = spacing(element).or(self.before),
                "spcAft" => self.after = spacing(element).or(self.after),
                "buNone" => self.bullet = Some(Bullet::None),
                "buChar" => self.bullet = Some(Bullet::Dots),
                "buAutoNum" => self.bullet = Some(Bullet::Numbers),
                "defRPr" => self.overlay_run(element, colours, theme),
                _ => {}
            }
        }
    }

    /// Puts on top what a run-properties element (`rPr`, `defRPr`) says.
    fn overlay_run(&mut self, node: Node, colours: &Colours, theme: &Theme) {
        if let Some(sz) = num(node, "sz") {
            self.size = Some(sz / 100.0);
        }
        self.bold = flag(node, "b").or(self.bold);
        self.italic = flag(node, "i").or(self.italic);
        if let Some(u) = node.attribute("u") {
            self.underline = Some(u != "none");
        }
        if let Some(s) = node.attribute("strike") {
            self.strike = Some(s != "noStrike");
        }
        if let Some(c) = node.attribute("cap") {
            self.caps = Some(match c {
                "all" => Caps::AllCaps,
                _ => Caps::None,
            });
        }
        for element in node.children().filter(Node::is_element) {
            match element.tag_name().name() {
                "solidFill" => self.colour = colours.inside(element).or(self.colour),
                "latin" => {
                    self.font = match element.attribute("typeface") {
                        Some("+mj-lt") => Some(theme.major.clone()),
                        Some("+mn-lt") => Some(theme.minor.clone()),
                        Some(name) if !name.is_empty() => Some(name.to_owned()),
                        _ => self.font.take(),
                    };
                }
                _ => {}
            }
        }
    }
}

/// The settings for paragraphs at a level (0 is the first), from containers of `lvlNpPr`
/// elements (a list style, a master's title or body style), the first the weakest.
fn level_from(containers: &[Node], level: usize, colours: &Colours, theme: &Theme) -> Level {
    let mut found = Level::default();
    let name = format!("lvl{}pPr", level + 1);
    for container in containers {
        if let Some(at) = child(*container, &name) {
            found.overlay_paragraph(at, colours, theme);
        }
    }
    found
}

// ---- reading -----------------------------------------------------------------------------

struct Reader<'p> {
    package: &'p Package,
    docs: RefCell<HashMap<String, Option<Doc<'p>>>>,
    /// Pictures already kept, by part name (and crop), so one used on every slide is kept once.
    kept: RefCell<HashMap<String, Option<Kept>>>,
}

impl<'p> Reader<'p> {
    fn doc(&self, path: &str) -> Option<Doc<'p>> {
        if let Some(found) = self.docs.borrow().get(path) {
            return found.clone();
        }
        let made = self.package.xml.get(path).and_then(|text| parse(text)).map(Rc::new);
        self.docs.borrow_mut().insert(path.to_owned(), made.clone());
        made
    }

    fn rels(&self, part: &str) -> Rels {
        let rels_path = format!("{}/_rels/{}.rels", folder_of(part), part.rsplit('/').next().unwrap_or(part));
        let rels_path = rels_path.trim_start_matches('/').to_owned();
        let Some(document) = self.doc(&rels_path) else { return Rels::new() };
        let mut rels = Rels::new();
        for rel in document.descendants().filter(|n| is(*n, "Relationship")) {
            if rel.attribute("TargetMode") == Some("External") {
                continue;
            }
            let (Some(id), Some(target)) = (rel.attribute("Id"), rel.attribute("Target")) else { continue };
            let kind = rel.attribute("Type").and_then(|t| t.rsplit('/').next()).unwrap_or("").to_owned();
            if let Some(path) = resolve(folder_of(part), target) {
                rels.insert(id.to_owned(), (kind, path));
            }
        }
        rels
    }
}

/// The part a relationship of this kind points at.
fn related(rels: &Rels, kind: &str) -> Option<String> {
    rels.values().find(|(k, _)| k == kind).map(|(_, path)| path.clone())
}

/// How the deck's slide sits on SoftPresenter's: scaled the same both ways, centred.
#[derive(Clone, Copy)]
struct Geometry {
    /// Slide units per EMU.
    scale: f64,
    dx: f64,
    dy: f64,
}

impl Geometry {
    fn new(cx: f64, cy: f64) -> Geometry {
        let (w, h) = (f64::from(sp_model::SLIDE_WIDTH), f64::from(sp_model::SLIDE_HEIGHT));
        let scale = (w / cx).min(h / cy);
        Geometry { scale, dx: (w - cx * scale) / 2.0, dy: (h - cy * scale) / 2.0 }
    }

    fn rect(&self, [x, y, w, h]: [f64; 4]) -> SlideRect {
        SlideRect {
            x: (x * self.scale + self.dx) as f32,
            y: (y * self.scale + self.dy) as f32,
            w: (w * self.scale) as f32,
            h: (h * self.scale) as f32,
        }
    }

    fn units(&self, emu: f64) -> f32 {
        (emu * self.scale) as f32
    }

    fn points(&self, points: f64) -> f32 {
        self.units(points * EMU_PER_POINT)
    }
}

/// A map from a group's child coordinates to the slide's, in EMU: `x' = x * sx + tx`.
#[derive(Clone, Copy)]
struct Place {
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
}

const IDENTITY: Place = Place { sx: 1.0, sy: 1.0, tx: 0.0, ty: 0.0 };

impl Place {
    fn apply(&self, [x, y, w, h]: [f64; 4]) -> [f64; 4] {
        [x * self.sx + self.tx, y * self.sy + self.ty, w * self.sx, h * self.sy]
    }
}

/// An `xfrm`'s offset and extent, in EMU.
fn box_of(xfrm: Node) -> Option<[f64; 4]> {
    let off = child(xfrm, "off")?;
    let ext = child(xfrm, "ext")?;
    Some([num(off, "x")?, num(off, "y")?, num(ext, "cx")?, num(ext, "cy")?])
}

fn xfrm_of<'a, 'b>(shape: Node<'a, 'b>) -> Option<Node<'a, 'b>> {
    let properties = child(shape, "spPr").or_else(|| child(shape, "grpSpPr"))?;
    child(properties, "xfrm")
}

fn name_of(shape: Node) -> String {
    shape
        .children()
        .filter(Node::is_element)
        .find_map(|c| child(c, "cNvPr"))
        .and_then(|n| n.attribute("name"))
        .unwrap_or("")
        .to_owned()
}

fn hidden(shape: Node) -> bool {
    shape.children().filter(Node::is_element).find_map(|c| child(c, "cNvPr")).and_then(|n| flag(n, "hidden"))
        == Some(true)
}

struct PlaceholderKey {
    kind: String,
    idx: Option<String>,
}

fn placeholder_of(shape: Node) -> Option<PlaceholderKey> {
    let properties = shape.children().filter(Node::is_element).find_map(|c| child(c, "nvPr"))?;
    let ph = child(properties, "ph")?;
    Some(PlaceholderKey {
        kind: ph.attribute("type").unwrap_or("obj").to_owned(),
        idx: ph.attribute("idx").map(str::to_owned),
    })
}

fn is_title(kind: &str) -> bool {
    matches!(kind, "title" | "ctrTitle")
}

/// The shape in a layout's or master's shape tree that a slide's placeholder stands in for.
fn find_placeholder<'a, 'b>(tree: Node<'a, 'b>, key: &PlaceholderKey, master: bool) -> Option<Node<'a, 'b>> {
    let shapes: Vec<(Node<'a, 'b>, PlaceholderKey)> =
        tree.children().filter_map(|s| Some((s, placeholder_of(s)?))).collect();
    if master {
        // A master has a title, a body and the footers.
        let want = if is_title(&key.kind) {
            "title"
        } else if matches!(key.kind.as_str(), "dt" | "ftr" | "sldNum") {
            key.kind.as_str()
        } else {
            "body"
        };
        return shapes.iter().find(|(_, o)| o.kind == want || (want == "title" && is_title(&o.kind))).map(|(s, _)| *s);
    }
    let same_kind = |o: &PlaceholderKey| o.kind == key.kind || (is_title(&key.kind) && is_title(&o.kind));
    shapes
        .iter()
        .find(|(_, o)| key.idx.is_some() && key.idx == o.idx && key.idx.as_deref() != Some("0"))
        .or_else(|| shapes.iter().find(|(_, o)| same_kind(o) && o.idx == key.idx))
        .or_else(|| shapes.iter().find(|(_, o)| same_kind(o)))
        .map(|(s, _)| *s)
}

fn tree_of<'a, 'b>(document: &'a Document<'b>) -> Option<Node<'a, 'b>> {
    document.descendants().find(|n| is(*n, "spTree"))
}

/// What a slide's shapes inherit from, and where its pictures are.
struct Parts<'x, 'p> {
    rels: &'x Rels,
    /// The presentation's default text style, and (for a placeholder) the master's style for
    /// its kind of text.
    default_style: Option<Node<'x, 'p>>,
    master_styles: Option<Node<'x, 'p>>,
    layout_tree: Option<Node<'x, 'p>>,
    master_tree: Option<Node<'x, 'p>>,
}

fn base_object(name: &str, frame: SlideRect) -> Object {
    Object { name: name.to_owned(), frame: Some(frame), fill: None, ..Object::shape(Shape::Rectangle) }
}

/// A picture kept: its name, and its size.
type Kept = (String, u32, u32);

type Save<'a> = dyn FnMut(&[u8], &str, Option<[f32; 4]>) -> Result<Kept, String> + 'a;

/// One slide being read.
struct SlideCx<'a, 'p> {
    reader: &'a Reader<'p>,
    geometry: Geometry,
    save: &'a mut Save<'a>,
    theme: &'a Theme,
    map: HashMap<String, String>,
    objects: Vec<Object>,
    flags: Vec<String>,
}

/// How a shape's text takes its look from the master.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Title,
    Body,
    Other,
}

fn style_of(key: &PlaceholderKey) -> Style {
    if is_title(&key.kind) {
        Style::Title
    } else if matches!(key.kind.as_str(), "dt" | "ftr" | "sldNum") {
        Style::Other
    } else {
        Style::Body
    }
}

/// A shape's line and fill, from its own properties or from the theme style it names.
struct Look {
    fill: Option<Fill>,
    stroke: Option<StrokeLook>,
}

/// What a frame or the newer form of an object holds, in a few words: a table, a chart, a
/// diagram or an embedded object.
fn frame_kind(frame: Node) -> &'static str {
    if frame.descendants().any(|n| is(n, "tbl")) {
        "a table"
    } else if frame.descendants().any(|n| is(n, "chart")) {
        "a chart"
    } else if frame.descendants().any(|n| is(n, "relIds")) {
        "a diagram"
    } else {
        "an embedded object"
    }
}

impl<'a, 'p> SlideCx<'a, 'p> {
    fn colours(&self) -> Colours<'_> {
        Colours { theme: self.theme, map: &self.map, placeholder: None }
    }

    fn flag(&mut self, why: &str) {
        if !self.flags.iter().any(|f| f == why) {
            self.flags.push(why.to_owned());
        }
    }

    /// Keeps a picture from the package, once however many slides show it.
    fn picture(&mut self, path: &str, crop: Option<[f32; 4]>) -> Option<Kept> {
        let key = format!("{path}|{crop:?}");
        if let Some(found) = self.reader.kept.borrow().get(&key) {
            return found.clone();
        }
        let saved = match self.reader.package.media.get(path) {
            Some(bytes) if !bytes.is_empty() => {
                let ending = path.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
                (self.save)(bytes, &ending, crop).ok()
            }
            _ => None,
        };
        self.reader.kept.borrow_mut().insert(key, saved.clone());
        saved
    }

    fn picture_object(&mut self, name: &str, frame: SlideRect, path: &str, crop: Option<[f32; 4]>) -> Option<Object> {
        let (file, _, _) = self.picture(path, crop)?;
        let mut object = base_object(name, frame);
        object.kind = ObjectKind::Image { file, fit: ImageFit::Stretch };
        Some(object)
    }

    /// Fills the whole slide with a background fill.
    fn background(&mut self, fill: Fill, rels: &Rels) {
        let whole = SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT };
        match fill {
            Fill::None => {}
            Fill::Solid(colour) => {
                let mut object = base_object("Background", whole);
                object.fill = Some(colour);
                self.objects.push(object);
            }
            Fill::Gradient(from, gradient) => {
                let mut object = base_object("Background", whole);
                object.fill = Some(from);
                object.gradient = Some(gradient);
                self.objects.push(object);
            }
            Fill::Picture(id) => {
                let path = rels.get(&id).map(|(_, p)| p.clone());
                match path.and_then(|p| self.picture_object("Background", whole, &p, None)) {
                    Some(object) => self.objects.push(object),
                    None => self.flag("a background picture couldn't be read"),
                }
            }
        }
    }

    /// The slide's background: its own, else its layout's, else its master's, else white.
    fn add_background(&mut self, sources: &[(&Document, &Rels)]) {
        for (document, rels) in sources {
            let Some(bg) = document.descendants().find(|n| is(*n, "bg")) else { continue };
            let fill = if let Some(properties) = child(bg, "bgPr") {
                read_fill(properties, &self.colours())
            } else if let Some(reference) = child(bg, "bgRef") {
                self.background_style(reference)
            } else {
                None
            };
            if let Some(fill) = fill {
                self.background(fill, rels);
                return;
            }
        }
        self.background(Fill::Solid(Rgba::WHITE), &Rels::new());
    }

    /// A `bgRef`: one of the theme's background fill styles, in the colour the reference names.
    fn background_style(&self, reference: Node) -> Option<Fill> {
        let idx = num(reference, "idx")? as usize;
        let placeholder = self.colours().inside(reference);
        let colours = Colours { theme: self.theme, map: &self.map, placeholder };
        let theme = self.reader.doc(&self.theme.path)?;
        let list = theme.descendants().find(|n| is(*n, "bgFillStyleLst"))?;
        let style = list.children().filter(Node::is_element).nth(idx.checked_sub(1001)?)?;
        read_fill_element(style, &colours).or_else(|| placeholder.map(Fill::Solid))
    }

    /// A shape's fill and line.
    fn look(&self, chain: &[Node], uses_style: bool) -> Look {
        let colours = self.colours();
        let fill = chain.iter().filter_map(|n| child(*n, "spPr")).find_map(|p| read_fill(p, &colours));
        let style = chain.first().and_then(|n| child(*n, "style")).filter(|_| uses_style);
        let fill = fill.or_else(|| {
            let reference = child(style?, "fillRef")?;
            (num(reference, "idx")? > 0.0).then(|| colours.inside(reference).map(Fill::Solid))?
        });
        let line = chain.iter().filter_map(|n| child(*n, "spPr")).find_map(|p| child(p, "ln"));
        let from_style = |wide: f64| {
            let reference = child(style?, "lnRef")?;
            (num(reference, "idx")? > 0.0).then(|| colours.inside(reference).map(|c| (wide, c)))?
        };
        let stroke = match line {
            Some(line) => {
                let width = num(line, "w").unwrap_or(9525.0);
                match read_fill(line, &colours) {
                    Some(Fill::Solid(colour)) => Some((width, colour)),
                    Some(_) => None,
                    None => from_style(width),
                }
            }
            None => from_style(9525.0),
        };
        Look { fill, stroke: stroke.map(|(w, color)| StrokeLook { width: self.geometry.units(w).max(1.0), color }) }
    }

    /// The shapes in a shape tree, in order, put through `place` (a group's change of
    /// coordinates).
    fn shapes(&mut self, tree: Node, parts: &Parts, place: Place, is_slide: bool, depth: usize) {
        if depth > MOST_GROUP_DEPTH {
            self.flag("shapes inside too many groups were left out");
            return;
        }
        for shape in tree.children().filter(Node::is_element) {
            self.one_shape(shape, parts, place, is_slide, depth);
        }
    }

    fn one_shape(&mut self, shape: Node, parts: &Parts, place: Place, is_slide: bool, depth: usize) {
        match shape.tag_name().name() {
            "sp" if !hidden(shape) => self.sp(shape, parts, place, is_slide),
            "pic" if !hidden(shape) => self.pic(shape, parts, place, is_slide),
            "cxnSp" if !hidden(shape) => self.line(shape, place),
            "grpSp" if !hidden(shape) => {
                let mut inner = place;
                if let Some(xfrm) = child(shape, "grpSpPr").and_then(|p| child(p, "xfrm"))
                    && let Some([x, y, w, h]) = box_of(xfrm)
                    && let (Some(off), Some(ext)) = (child(xfrm, "chOff"), child(xfrm, "chExt"))
                {
                    let (ox, oy) = (num(off, "x").unwrap_or(0.0), num(off, "y").unwrap_or(0.0));
                    let (ew, eh) = (num(ext, "cx").unwrap_or(0.0), num(ext, "cy").unwrap_or(0.0));
                    let (sx, sy) = (if ew > 0.0 { w / ew } else { 1.0 }, if eh > 0.0 { h / eh } else { 1.0 });
                    let own = Place { sx, sy, tx: x - ox * sx, ty: y - oy * sy };
                    inner = Place {
                        sx: place.sx * own.sx,
                        sy: place.sy * own.sy,
                        tx: place.sx * own.tx + place.tx,
                        ty: place.sy * own.ty + place.ty,
                    };
                }
                self.shapes(shape, parts, inner, is_slide, depth + 1);
            }
            "graphicFrame" if !hidden(shape) => self.frame(shape, parts, place),
            "AlternateContent" => self.alternate(shape, parts, place, is_slide, depth),
            _ => {}
        }
    }

    /// `mc:AlternateContent`: the choice if everything in it can be drawn, else its fallback.
    fn alternate(&mut self, content: Node, parts: &Parts, place: Place, is_slide: bool, depth: usize) {
        let simple = |n: Node| matches!(n.tag_name().name(), "sp" | "pic" | "cxnSp" | "grpSp");
        let use_choice = child(content, "Choice").filter(|c| c.children().filter(Node::is_element).all(simple));
        match (use_choice, child(content, "Fallback")) {
            (Some(c), _) => self.shapes(c, parts, place, is_slide, depth + 1),
            (None, Some(f)) => {
                // What the object was, from what the newer form of it held.
                let what = child(content, "Choice").map_or("something", frame_kind);
                self.flag(&format!("{what} came in as its picture"));
                self.shapes(f, parts, place, is_slide, depth + 1);
            }
            (None, None) => self.flag("something couldn't come in"),
        }
    }

    /// A chart, table, diagram or embedded object: its picture if the file keeps one.
    fn frame(&mut self, frame: Node, parts: &Parts, place: Place) {
        let what = frame_kind(frame);

        let Some(boxed) = child(frame, "xfrm").and_then(box_of) else {
            self.flag(&format!("{what} couldn't come in"));
            return;
        };
        let embed = frame
            .descendants()
            .filter(|n| is(*n, "blip"))
            .find_map(|b| r_attr(b, "embed").and_then(|id| parts.rels.get(id)).map(|(_, p)| p.clone()));
        let rect = self.geometry.rect(place.apply(boxed));
        match embed.and_then(|path| self.picture_object(what, rect, &path, None)) {
            Some(object) => {
                self.objects.push(object);
                self.flag(&format!("{what} came in as its picture"));
            }
            None => self.flag(&format!("{what} couldn't come in")),
        }
    }

    fn pic(&mut self, shape: Node, parts: &Parts, place: Place, is_slide: bool) {
        let key = placeholder_of(shape);
        let mut chain = vec![shape];
        if is_slide && let Some(key) = &key {
            chain.extend(parts.layout_tree.and_then(|t| find_placeholder(t, key, false)));
        }
        let Some(boxed) = chain.iter().find_map(|n| xfrm_of(*n).and_then(box_of)) else { return };
        let rect = self.geometry.rect(place.apply(boxed));
        let fill = child(shape, "blipFill");
        let path = fill
            .and_then(|f| child(f, "blip"))
            .and_then(|b| r_attr(b, "embed"))
            .and_then(|id| parts.rels.get(id))
            .map(|(_, p)| p.clone());
        let crop = fill.and_then(|f| child(f, "srcRect")).and_then(|r| {
            let side = |name: &str| (num(r, name).unwrap_or(0.0) / 100_000.0).clamp(0.0, 1.0) as f32;
            let c = [side("l"), side("t"), side("r"), side("b")];
            (c.iter().any(|v| *v > 0.0) && c[0] + c[2] < 1.0 && c[1] + c[3] < 1.0).then_some(c)
        });
        if shape.descendants().any(|n| matches!(n.tag_name().name(), "videoFile" | "audioFile" | "wavAudioFile")) {
            self.flag("embedded sound or video didn't come in");
        }
        let name = name_of(shape);
        let Some(mut object) = path.and_then(|p| self.picture_object(&name, rect, &p, crop)) else {
            self.flag("a picture couldn't be read");
            return;
        };
        if let Some(own) = xfrm_of(shape) {
            object.rotation = (num(own, "rot").unwrap_or(0.0) / 60_000.0) as f32;
            object.flip_h = flag(own, "flipH") == Some(true);
            object.flip_v = flag(own, "flipV") == Some(true);
        }
        object.stroke = self.look(&[shape], false).stroke;
        self.objects.push(object);
    }

    /// A connector or line: a thin box, turned.
    fn line(&mut self, shape: Node, place: Place) {
        let Some(xfrm) = xfrm_of(shape) else { return };
        let Some(boxed) = box_of(xfrm).map(|b| place.apply(b)) else { return };
        let Some(stroke) = self.look(&[shape], true).stroke else { return };
        let area = self.geometry.rect(boxed);
        let (dx, dy) = (f64::from(area.w), f64::from(area.h));
        let length = dx.hypot(dy).max(1.0);
        let mut angle = dy.atan2(dx).to_degrees();
        if (flag(xfrm, "flipH") == Some(true)) != (flag(xfrm, "flipV") == Some(true)) {
            angle = -angle;
        }
        let (mx, my) = (f64::from(area.x) + dx / 2.0, f64::from(area.y) + dy / 2.0);
        let frame = SlideRect {
            x: (mx - length / 2.0) as f32,
            y: (my - f64::from(stroke.width) / 2.0) as f32,
            w: length as f32,
            h: stroke.width,
        };
        let mut object = base_object(&name_of(shape), frame);
        object.fill = Some(stroke.color);
        object.rotation = angle as f32;
        self.objects.push(object);
    }

    fn sp(&mut self, shape: Node, parts: &Parts, place: Place, is_slide: bool) {
        let key = placeholder_of(shape);
        if key.as_ref().is_some_and(|k| matches!(k.kind.as_str(), "sldNum" | "dt")) {
            return;
        }
        // The shape, then the layout's and the master's for the same placeholder.
        let mut chain = vec![shape];
        if let Some(key) = &key
            && is_slide
        {
            chain.extend(parts.layout_tree.and_then(|t| find_placeholder(t, key, false)));
            chain.extend(parts.master_tree.and_then(|t| find_placeholder(t, key, true)));
        }
        let Some(boxed) = chain.iter().find_map(|n| xfrm_of(*n).and_then(box_of)) else { return };
        let frame = self.geometry.rect(place.apply(boxed));
        let own_xfrm = xfrm_of(shape);
        let rotation = (own_xfrm.and_then(|x| num(x, "rot")).unwrap_or(0.0) / 60_000.0) as f32;
        let flip_h = own_xfrm.and_then(|x| flag(x, "flipH")) == Some(true);
        let flip_v = own_xfrm.and_then(|x| flag(x, "flipV")) == Some(true);
        let preset = chain
            .iter()
            .find_map(|n| child(*n, "spPr").and_then(|p| child(p, "prstGeom")))
            .and_then(|g| g.attribute("prst"))
            .unwrap_or("rect");
        let custom = chain.iter().any(|n| child(*n, "spPr").is_some_and(|p| child(p, "custGeom").is_some()));
        let text_box = key.is_some()
            || child(shape, "nvSpPr").and_then(|n| child(n, "cNvSpPr")).and_then(|c| flag(c, "txBox")) == Some(true);
        let look = self.look(&chain, key.is_none());
        let name = name_of(shape);
        let drawn = match preset {
            "rect" | "flowChartProcess" => Some(Shape::Rectangle),
            "roundRect" => Some(Shape::RoundedRect),
            "ellipse" | "flowChartConnector" => Some(Shape::Ellipse),
            "triangle" => Some(Shape::Triangle),
            "rtTriangle" => Some(Shape::RightTriangle),
            "diamond" => Some(Shape::Rhombus),
            "pentagon" | "hexagon" | "octagon" => Some(Shape::Polygon),
            "star5" => Some(Shape::Star),
            "rightArrow" => Some(Shape::Arrow),
            "leftRightArrow" => Some(Shape::DoubleArrow),
            _ => None,
        }
        .filter(|_| !custom);
        let split = |look: &Look| match &look.fill {
            Some(Fill::Solid(c)) => (Some(*c), None),
            Some(Fill::Gradient(from, g)) => (Some(*from), Some(*g)),
            _ => (None, None),
        };
        let visible = split(&look).0.is_some() || look.stroke.is_some();
        let (mut box_fill, mut box_gradient, mut box_stroke) = (None, None, None);
        if text_box && (drawn == Some(Shape::Rectangle) || drawn.is_none()) {
            // A box: its fill and line go on the text itself.
            (box_fill, box_gradient) = split(&look);
            box_stroke = look.stroke;
        } else if visible {
            match drawn {
                Some(kind) => {
                    let mut object = base_object(&name, frame);
                    (object.fill, object.gradient) = split(&look);
                    object.stroke = look.stroke;
                    object.rotation = rotation;
                    object.flip_h = flip_h;
                    object.flip_v = flip_v;
                    object.kind = ObjectKind::Shape { shape: kind };
                    self.objects.push(object);
                }
                None => self.flag("a shape couldn't be drawn"),
            }
        }
        if let Some(mut text) = self.text(&chain, key.as_ref(), parts, frame) {
            text.name = name;
            text.rotation = rotation;
            text.fill = box_fill;
            text.gradient = box_gradient;
            text.stroke = box_stroke;
            self.objects.push(text);
        }
    }

    /// A shape's words as a text object, or `None` when it has none.
    fn text(
        &mut self,
        chain: &[Node],
        key: Option<&PlaceholderKey>,
        parts: &Parts,
        frame: SlideRect,
    ) -> Option<Object> {
        let body = child(chain[0], "txBody")?;
        // The weakest style first: the deck's, the master's for this kind of text, then the
        // master's, layout's and shape's own list styles.
        let mut containers: Vec<Node> = Vec::new();
        containers.extend(parts.default_style);
        if let Some(key) = key
            && let Some(styles) = parts.master_styles
        {
            let name = match style_of(key) {
                Style::Title => "titleStyle",
                Style::Body => "bodyStyle",
                Style::Other => "otherStyle",
            };
            containers.extend(child(styles, name));
        }
        for node in chain.iter().rev() {
            if let Some(list) = child(*node, "txBody").and_then(|b| child(b, "lstStyle")) {
                containers.push(list);
            }
        }
        let theme = self.theme;
        let colours = Colours { theme, map: &self.map, placeholder: None };
        let font_ref = key
            .is_none()
            .then(|| child(chain[0], "style").and_then(|s| child(s, "fontRef")))
            .flatten()
            .and_then(|r| colours.inside(r));

        let mut lines: Vec<String> = Vec::new();
        let mut kinds: Vec<(Level, usize)> = Vec::new();
        let mut first: Option<Level> = None;
        let (mut dots, mut numbers, mut counted) = (0, 0, 0);
        let (mut spaced, mut spaced_n) = (0.0_f64, 0);
        for paragraph in children(body, "p") {
            let properties = child(paragraph, "pPr");
            let at = properties.and_then(|p| num(p, "lvl")).unwrap_or(0.0).clamp(0.0, 8.0) as usize;
            let mut level = level_from(&containers, at, &colours, theme);
            if font_ref.is_some() {
                level.colour = font_ref;
            }
            if let Some(properties) = properties {
                level.overlay_paragraph(properties, &colours, theme);
            }
            let mut line = String::new();
            let mut lined: Vec<String> = Vec::new();
            for element in paragraph.children().filter(Node::is_element) {
                match element.tag_name().name() {
                    "r" | "fld" => {
                        let field = element.tag_name().name() == "fld";
                        if field
                            && element
                                .attribute("type")
                                .is_some_and(|t| t.starts_with("slidenum") || t.starts_with("datetime"))
                        {
                            continue;
                        }
                        let text: String = children(element, "t").filter_map(|t| t.text()).collect();
                        let mut run = level.clone();
                        if let Some(rpr) = child(element, "rPr") {
                            run.overlay_run(rpr, &colours, theme);
                        }
                        let letters = text.chars().filter(|c| !c.is_whitespace()).count();
                        if letters > 0 {
                            match kinds.iter_mut().find(|(k, _)| k.same_look(&run)) {
                                Some((_, n)) => *n += letters,
                                None => kinds.push((run, letters)),
                            }
                        }
                        line.push_str(&text);
                    }
                    "br" => lined.push(std::mem::take(&mut line)),
                    _ => {}
                }
            }
            lined.push(line);
            if lined.iter().any(|l| !l.trim().is_empty()) {
                let size = level.size.unwrap_or(18.0);
                first.get_or_insert_with(|| level.clone());
                counted += 1;
                match level.bullet {
                    Some(Bullet::Dots) => dots += 1,
                    Some(Bullet::Numbers) => numbers += 1,
                    _ => {}
                }
                let each = |spacing: Option<(bool, f64)>| match spacing {
                    Some((true, share)) => share * size * SINGLE,
                    Some((false, points)) => points,
                    None => 0.0,
                };
                spaced += each(level.before) + each(level.after);
                spaced_n += 1;
            }
            lines.extend(lined);
        }
        let first = first?;
        // Blank lines at either end say nothing.
        while lines.last().is_some_and(|l| l.trim().is_empty()) {
            lines.pop();
        }
        let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        let words = lines[start..].join("\n");
        let dominant = kinds.iter().max_by_key(|(_, n)| *n).map(|(k, _)| k.clone()).unwrap_or_else(|| first.clone());
        let tenths = |k: &Level| k.size.map(|s| (s * 10.0) as i64);
        if kinds
            .iter()
            .any(|(k, _)| (tenths(k), k.colour, &k.font) != (tenths(&dominant), dominant.colour, &dominant.font))
        {
            self.flag("a text box had mixed sizes; it took its main look");
        }

        // The box: insets, anchor and fitting, from the shape or what it inherits.
        let mut inset = [91_440.0, 45_720.0, 91_440.0, 45_720.0];
        let (mut anchor, mut fit, mut scale) = (None, None::<&str>, 1.0);
        for node in chain.iter().rev() {
            let Some(properties) = child(*node, "txBody").and_then(|b| child(b, "bodyPr")) else { continue };
            for (at, name) in ["lIns", "tIns", "rIns", "bIns"].iter().enumerate() {
                if let Some(v) = num(properties, name) {
                    inset[at] = v;
                }
            }
            anchor = properties.attribute("anchor").or(anchor);
            let fitting = properties
                .children()
                .filter(Node::is_element)
                .find(|c| matches!(c.tag_name().name(), "normAutofit" | "noAutofit" | "spAutoFit"));
            if let Some(f) = fitting {
                fit = Some(f.tag_name().name());
                scale = num(f, "fontScale").map_or(1.0, |s| s / 100_000.0);
            }
        }

        let size = dominant.size.unwrap_or(18.0) * scale;
        let line_height = match first.line {
            Some((true, share)) => (share * SINGLE) as f32,
            Some((false, points)) => (points / size.max(1.0)) as f32,
            None => SINGLE as f32,
        };
        let font = dominant
            .font
            .clone()
            .filter(|f| !f.is_empty())
            .or_else(|| Some(theme.minor.clone()).filter(|f| !f.is_empty()));
        let g = self.geometry;
        let style = TextStyle {
            font_family: font,
            size: Some(g.points(size).max(1.0)),
            weight: Some(if dominant.bold == Some(true) { 700 } else { 400 }),
            italic: Some(dominant.italic == Some(true)),
            fill: Some(dominant.colour.unwrap_or(Rgba::BLACK)),
            align: Some(first.align.unwrap_or(Align::Left)),
            outline_width: Some(0.0),
            shadow: Some(false),
            valign: Some(match anchor {
                Some("ctr") => VAlign::Middle,
                Some("b") => VAlign::Bottom,
                _ => VAlign::Top,
            }),
            caps: Some(dominant.caps.unwrap_or(Caps::None)),
            underline: Some(dominant.underline == Some(true)),
            strike: Some(dominant.strike == Some(true)),
            scaling: Some(if fit == Some("normAutofit") { Scaling::TextDown } else { Scaling::None }),
            line_height: Some(line_height),
            letter_spacing: Some(0.0),
            margins: Some([g.units(inset[0]), g.units(inset[1]), g.units(inset[2]), g.units(inset[3])]),
            list: Some(if counted > 0 && dots * 2 > counted {
                ListStyle::Bullets
            } else if counted > 0 && numbers * 2 > counted {
                ListStyle::Numbers
            } else {
                ListStyle::None
            }),
            paragraph_spacing: Some(if spaced_n > 0 { g.points(spaced / f64::from(spaced_n)) } else { 0.0 }),
            baseline: Some(0.0),
            ..TextStyle::default()
        };
        let mut object = base_object("Text", frame);
        object.kind = ObjectKind::Text { text: words, style };
        Some(object)
    }
}

impl Level {
    /// Whether two runs look the same.
    fn same_look(&self, other: &Level) -> bool {
        (self.size, self.bold, self.italic, self.underline, self.strike, self.colour, &self.font, self.caps)
            == (
                other.size,
                other.bold,
                other.italic,
                other.underline,
                other.strike,
                other.colour,
                &other.font,
                other.caps,
            )
    }
}

/// The notes on a slide's notes page: the words of its body placeholder.
fn notes_of(reader: &Reader, rels: &Rels) -> Option<String> {
    let path = related(rels, "notesSlide")?;
    let document = reader.doc(&path)?;
    let tree = tree_of(&document)?;
    let body =
        tree.children().filter(|s| is(*s, "sp")).find(|s| placeholder_of(*s).is_some_and(|k| k.kind == "body"))?;
    let lines: Vec<String> = children(child(body, "txBody")?, "p")
        .map(|p| p.descendants().filter(|n| is(*n, "t")).filter_map(|t| t.text()).collect::<String>())
        .collect();
    let text = lines.join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The colour mapping a master says (`clrMap`) or a layout or slide overrides.
fn colour_map(document: Option<&Document>, over: bool) -> Option<HashMap<String, String>> {
    let document = document?;
    let name = if over { "overrideClrMapping" } else { "clrMap" };
    let element = document.descendants().find(|n| is(*n, name))?;
    Some(element.attributes().map(|a| (a.name().to_owned(), a.value().to_owned())).collect())
}

/// Reads a `.pptx` into slides. `save` keeps a picture and says the name it was kept under
/// and its size; `most` is the most slides read.
pub fn read(bytes: &[u8], name: &str, most: usize, save: &mut Save<'_>) -> Result<Read, String> {
    let package = open_package(bytes, name)?;
    let reader = Reader { package: &package, docs: RefCell::new(HashMap::new()), kept: RefCell::new(HashMap::new()) };
    let presentation =
        reader.doc("ppt/presentation.xml").ok_or_else(|| format!("{name} isn't a .pptx SoftPresenter can read."))?;
    let presentation_rels = reader.rels("ppt/presentation.xml");
    let size = presentation.descendants().find(|n| is(*n, "sldSz"));
    let (cx, cy) = size
        .and_then(|s| Some((num(s, "cx")?, num(s, "cy")?)))
        .filter(|(w, h)| *w > 0.0 && *h > 0.0)
        .unwrap_or((9_144_000.0, 6_858_000.0));
    let geometry = Geometry::new(cx, cy);
    let default_style = presentation.descendants().find(|n| is(*n, "defaultTextStyle"));
    let mut result = Read::default();
    let list = presentation.descendants().find(|n| is(*n, "sldIdLst"));
    let slide_paths: Vec<String> = list
        .map(|l| {
            children(l, "sldId")
                .filter_map(|s| presentation_rels.get(r_attr(s, "id")?).map(|(_, p)| p.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut themes: HashMap<String, Rc<Theme>> = HashMap::new();
    for (at, path) in slide_paths.iter().take(most).enumerate() {
        let number = at + 1;
        let slide_rels = reader.rels(path);
        let layout_path = related(&slide_rels, "slideLayout");
        let layout_doc = layout_path.as_deref().and_then(|p| reader.doc(p));
        let layout_rels = layout_path.as_deref().map(|p| reader.rels(p)).unwrap_or_default();
        let master_path = related(&layout_rels, "slideMaster");
        let master_doc = master_path.as_deref().and_then(|p| reader.doc(p));
        let master_rels = master_path.as_deref().map(|p| reader.rels(p)).unwrap_or_default();
        let theme_path = related(&master_rels, "theme").unwrap_or_default();
        let theme = themes
            .entry(theme_path.clone())
            .or_insert_with(|| {
                let mut theme = reader.doc(&theme_path).map(|d| read_theme(&d)).unwrap_or_default();
                theme.path = theme_path.clone();
                Rc::new(theme)
            })
            .clone();

        let Some(slide_doc) = reader.doc(path) else {
            result.slides.push(Slide::new(Vec::new()));
            result.check.push((number, vec!["the slide couldn't be read".to_owned()]));
            continue;
        };
        let mut map = colour_map(master_doc.as_deref(), false).unwrap_or_default();
        for over in [colour_map(layout_doc.as_deref(), true), colour_map(Some(&slide_doc), true)].into_iter().flatten()
        {
            map.extend(over);
        }
        let mut cx = SlideCx {
            reader: &reader,
            geometry,
            save: &mut *save,
            theme: &theme,
            map,
            objects: Vec::new(),
            flags: Vec::new(),
        };

        let mut backgrounds: Vec<(&Document, &Rels)> = vec![(&slide_doc, &slide_rels)];
        if let Some(d) = layout_doc.as_deref() {
            backgrounds.push((d, &layout_rels));
        }
        if let Some(d) = master_doc.as_deref() {
            backgrounds.push((d, &master_rels));
        }
        cx.add_background(&backgrounds);

        let master_tree = master_doc.as_deref().and_then(tree_of);
        let layout_tree = layout_doc.as_deref().and_then(tree_of);
        let master_styles = master_doc.as_deref().and_then(|d| d.descendants().find(|n| is(*n, "txStyles")));
        let shows = |d: Option<&Document>| d.and_then(|d| flag(d.root_element(), "showMasterSp")).unwrap_or(true);
        let slide_shows = shows(Some(&slide_doc));
        let layout_shows = shows(layout_doc.as_deref());
        // Pictures and shapes of the master and layout sit behind the slide's own.
        for (tree, rels, shown) in
            [(master_tree, &master_rels, slide_shows && layout_shows), (layout_tree, &layout_rels, slide_shows)]
        {
            let Some(tree) = tree.filter(|_| shown) else { continue };
            let parts = Parts { rels, default_style, master_styles, layout_tree: None, master_tree: None };
            for shape in tree.children().filter(Node::is_element) {
                if placeholder_of(shape).is_none() {
                    cx.one_shape(shape, &parts, IDENTITY, false, 0);
                }
            }
        }
        let parts = Parts { rels: &slide_rels, default_style, master_styles, layout_tree, master_tree };
        if let Some(tree) = tree_of(&slide_doc) {
            cx.shapes(tree, &parts, IDENTITY, true, 0);
        }
        let flags = std::mem::take(&mut cx.flags);
        let mut slide = Slide::new(std::mem::take(&mut cx.objects));
        slide.enabled = flag(slide_doc.root_element(), "show") != Some(false);
        slide.notes = notes_of(&reader, &slide_rels);
        result.slides.push(slide);
        if !flags.is_empty() {
            result.check.push((number, flags));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
        xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
        xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" \
        xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"";
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    type TestSlide<'a> = (String, String, Vec<(&'a str, &'a str, &'a str)>);

    fn zipped(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in parts {
            writer.start_file(name.as_str(), options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn part(name: &str, body: &str) -> (String, Vec<u8>) {
        (name.to_owned(), format!("<?xml version=\"1.0\"?>{body}").into_bytes())
    }

    fn rels(of: &str, links: &[(&str, &str, &str)]) -> (String, Vec<u8>) {
        let folder = folder_of(of);
        let file = of.rsplit('/').next().unwrap();
        let list: String = links
            .iter()
            .map(|(id, kind, target)| format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"/>"))
            .collect();
        part(&format!("{folder}/_rels/{file}.rels"), &format!("<Relationships>{list}</Relationships>"))
    }

    fn picture_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[200; 12]).unwrap();
        out
    }

    fn text_shape(tag: &str, ph: &str, xfrm: &str, body: &str) -> String {
        format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"{tag}\"/><p:cNvSpPr/><p:nvPr>{ph}</p:nvPr></p:nvSpPr>\
             <p:spPr>{xfrm}</p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp>"
        )
    }

    fn xfrm(x: i64, y: i64, w: i64, h: i64) -> String {
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>")
    }

    fn slide(extra_attrs: &str, background: &str, shapes: &str) -> String {
        format!(
            "<p:sld {NS} {extra_attrs}><p:cSld>{background}<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/>\
             <p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld></p:sld>"
        )
    }

    /// The parts every deck here has: a master with a title and a body (a bullet list), a layout
    /// that moves the title, and a theme.
    fn frame(slides: &[TestSlide], extra: Vec<(String, Vec<u8>)>) -> Vec<u8> {
        let ids: String =
            (0..slides.len()).map(|n| format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 256 + n, n + 2)).collect();
        let mut parts = vec![
            part(
                "ppt/presentation.xml",
                &format!(
                    "<p:presentation {NS}><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"9144000\" cy=\"5143500\"/>\
                     <p:defaultTextStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:defaultTextStyle></p:presentation>"
                ),
            ),
            part(
                "ppt/slideMasters/slideMaster1.xml",
                &format!(
                    "<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg>\
                     <p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}{}</p:spTree></p:cSld>\
                     <p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\"/>\
                     <p:txStyles><p:titleStyle><a:lvl1pPr algn=\"ctr\"><a:defRPr sz=\"4400\" b=\"1\"><a:solidFill><a:schemeClr val=\"tx2\"/></a:solidFill>\
                     <a:latin typeface=\"+mj-lt\"/></a:defRPr></a:lvl1pPr></p:titleStyle>\
                     <p:bodyStyle><a:lvl1pPr marL=\"342900\" algn=\"l\"><a:buChar char=\"&#8226;\"/><a:defRPr sz=\"2800\"><a:solidFill><a:schemeClr val=\"tx1\"/></a:solidFill>\
                     <a:latin typeface=\"+mn-lt\"/></a:defRPr></a:lvl1pPr></p:bodyStyle><p:otherStyle/></p:txStyles></p:sldMaster>",
                    text_shape(
                        "Title",
                        "<p:ph type=\"title\"/>",
                        &xfrm(457200, 205979, 8229600, 857250),
                        "<a:p><a:endParaRPr/></a:p>"
                    )
                    .replace("<a:bodyPr/>", "<a:bodyPr anchor=\"ctr\"><a:normAutofit/></a:bodyPr>"),
                    text_shape(
                        "Body",
                        "<p:ph type=\"body\" idx=\"1\"/>",
                        &xfrm(457200, 1200150, 8229600, 3394472),
                        "<a:p><a:endParaRPr/></a:p>"
                    ),
                ),
            ),
            rels("ppt/slideMasters/slideMaster1.xml", &[("rId1", "theme", "../theme/theme1.xml")]),
            part(
                "ppt/slideLayouts/slideLayout1.xml",
                &format!(
                    "<p:sldLayout {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}{}</p:spTree></p:cSld></p:sldLayout>",
                    text_shape(
                        "Title",
                        "<p:ph type=\"title\"/>",
                        &xfrm(914400, 300000, 7315200, 700000),
                        "<a:p><a:endParaRPr/></a:p>"
                    ),
                    text_shape("Body", "<p:ph idx=\"1\"/>", "", "<a:p><a:endParaRPr/></a:p>"),
                ),
            ),
            rels("ppt/slideLayouts/slideLayout1.xml", &[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]),
            part(
                "ppt/theme/theme1.xml",
                "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:themeElements><a:clrScheme name=\"t\">\
                 <a:dk1><a:sysClr val=\"windowText\" lastClr=\"000000\"/></a:dk1><a:lt1><a:sysClr val=\"window\" lastClr=\"FFFFFF\"/></a:lt1>\
                 <a:dk2><a:srgbClr val=\"1F497D\"/></a:dk2><a:lt2><a:srgbClr val=\"EEECE1\"/></a:lt2>\
                 <a:accent1><a:srgbClr val=\"4F81BD\"/></a:accent1></a:clrScheme>\
                 <a:fontScheme name=\"f\"><a:majorFont><a:latin typeface=\"Georgia\"/></a:majorFont><a:minorFont><a:latin typeface=\"Verdana\"/></a:minorFont></a:fontScheme>\
                 <a:fmtScheme name=\"x\"><a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>",
            ),
        ];
        let mut links = vec![("rId1", "slideMaster", "slideMasters/slideMaster1.xml")];
        let paths: Vec<String> = (1..=slides.len()).map(|n| format!("slides/slide{n}.xml")).collect();
        let ids: Vec<String> = (0..slides.len()).map(|n| format!("rId{}", n + 2)).collect();
        for (n, path) in paths.iter().enumerate() {
            links.push((ids[n].as_str(), "slide", path.as_str()));
        }
        parts.push(rels("ppt/presentation.xml", &links));
        for (n, (_, xml, targets)) in slides.iter().enumerate() {
            let name = format!("ppt/slides/slide{}.xml", n + 1);
            parts.push((name.clone(), format!("<?xml version=\"1.0\"?>{xml}").into_bytes()));
            let mut all = vec![("rIdL", "slideLayout", "../slideLayouts/slideLayout1.xml")];
            all.extend(targets.iter().copied());
            parts.push(rels(&name, &all));
        }
        parts.extend(extra);
        zipped(&parts)
    }

    fn numbered_saver() -> impl FnMut(&[u8], &str, Option<[f32; 4]>) -> Result<Kept, String> {
        let mut count = 0;
        move |_, ending, _| {
            count += 1;
            Ok((format!("picture-{count}.{ending}"), 2, 2))
        }
    }

    fn sample() -> Vec<u8> {
        let chart_frame = "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"5\" name=\"Chart\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>\
            <p:xfrm><a:off x=\"1000000\" y=\"1000000\"/><a:ext cx=\"4000000\" cy=\"2000000\"/></p:xfrm>\
            <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"c\" r:id=\"rId9\"/></a:graphicData></a:graphic></p:graphicFrame>";
        let fallback_picture = "<p:pic><p:nvPicPr><p:cNvPr id=\"6\" name=\"Chart picture\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>\
            <p:blipFill><a:blip r:embed=\"rId2\"/></p:blipFill><p:spPr><a:xfrm><a:off x=\"1000000\" y=\"1000000\"/><a:ext cx=\"4000000\" cy=\"2000000\"/></a:xfrm></p:spPr></p:pic>";
        let slides = vec![
            // 1: a title and a body, each taking its place and look from the layout and master.
            (
                String::new(),
                slide(
                    "",
                    "",
                    &format!(
                        "{}{}",
                        text_shape("Title 1", "<p:ph type=\"title\"/>", "", "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Welcome</a:t></a:r></a:p>"),
                        text_shape(
                            "Content 2",
                            "<p:ph idx=\"1\"/>",
                            "",
                            "<a:p><a:r><a:t>First point</a:t></a:r></a:p><a:p><a:r><a:t>Second point</a:t></a:r></a:p>"
                        ),
                    ),
                ),
                vec![],
            ),
            // 2: a picture background, a text box, a rectangle.
            (
                String::new(),
                slide(
                    "",
                    "<p:bg><p:bgPr><a:blipFill><a:blip r:embed=\"rId2\"/></a:blipFill></p:bgPr></p:bg>",
                    &format!(
                        "{}<p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"Box\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"/>\
                         <a:solidFill><a:srgbClr val=\"FF0000\"/></a:solidFill><a:ln w=\"19050\"><a:solidFill><a:srgbClr val=\"00FF00\"/></a:solidFill></a:ln></p:spPr></p:sp>",
                        text_shape(
                            "TextBox 3",
                            "",
                            &xfrm(500000, 500000, 3000000, 800000),
                            "<a:p><a:pPr algn=\"r\"><a:lnSpc><a:spcPct val=\"150000\"/></a:lnSpc></a:pPr><a:r><a:rPr sz=\"2000\" b=\"1\" i=\"1\" u=\"sng\"><a:solidFill><a:srgbClr val=\"FF8800\"/></a:solidFill><a:latin typeface=\"Tahoma\"/></a:rPr><a:t>Hello</a:t></a:r>\
                             <a:r><a:rPr sz=\"3600\"/><a:t> yo</a:t></a:r></a:p>"
                        )
                        .replace("<p:cNvSpPr/>", "<p:cNvSpPr txBox=\"1\"/>"),
                        xfrm(1000000, 3000000, 2000000, 1000000),
                    ),
                ),
                vec![("rId2", "image", "../media/image1.png")],
            ),
            // 3: a chart with the picture the file keeps for it.
            (
                String::new(),
                slide(
                    "",
                    "",
                    &format!("<mc:AlternateContent><mc:Choice Requires=\"c14\">{chart_frame}</mc:Choice><mc:Fallback>{fallback_picture}</mc:Fallback></mc:AlternateContent>"),
                ),
                vec![("rId2", "image", "../media/image1.png")],
            ),
            // 4: hidden, with a chart that keeps no picture.
            (String::new(), slide("show=\"0\"", "", chart_frame), vec![]),
            // 5: with notes.
            (String::new(), slide("", "", ""), vec![("rIdN", "notesSlide", "../notesSlides/notesSlide1.xml")]),
        ];
        let notes = part(
            "ppt/notesSlides/notesSlide1.xml",
            &format!(
                "<p:notes {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}{}</p:spTree></p:cSld></p:notes>",
                text_shape("Slide image", "<p:ph type=\"sldImg\"/>", "", ""),
                text_shape(
                    "Notes",
                    "<p:ph type=\"body\" idx=\"1\"/>",
                    "",
                    "<a:p><a:r><a:t>Remember to pray.</a:t></a:r></a:p><a:p><a:r><a:t>Then the offering.</a:t></a:r></a:p>"
                ),
            ),
        );
        frame(&slides, vec![notes, ("ppt/media/image1.png".to_owned(), picture_bytes())])
    }

    fn text_of(object: &Object) -> (&str, &TextStyle) {
        match &object.kind {
            ObjectKind::Text { text, style } => (text, style),
            other => panic!("not text: {other:?}"),
        }
    }

    #[test]
    fn a_placeholder_takes_its_place_and_look_from_the_layout_and_master_and_theme() {
        let read = read(&sample(), "Sample.pptx", 300, &mut numbered_saver()).unwrap();
        assert_eq!(read.slides.len(), 5);
        let first = &read.slides[0];
        // A white background (theme colour through the master's background style), then the title,
        // then the body.
        assert_eq!(first.objects.len(), 3, "{:?}", first.objects);
        assert_eq!(first.objects[0].fill, Some(Rgba::WHITE));
        let (words, title) = text_of(&first.objects[1]);
        assert_eq!(words, "Welcome");
        // The slide is 9144000 EMU wide: 1920 / 9144000 units per EMU, and 12700 EMU a point.
        let unit = 1920.0 / 9144000.0;
        let frame = first.objects[1].frame.unwrap();
        // The layout's place for the title wins over the master's.
        assert!(
            (f64::from(frame.x) - 914400.0 * unit).abs() < 0.01 && (f64::from(frame.w) - 7315200.0 * unit).abs() < 0.01
        );
        assert!((f64::from(title.size.unwrap()) - 44.0 * 12700.0 * unit).abs() < 0.01);
        assert_eq!(title.weight, Some(700));
        assert_eq!(title.align, Some(Align::Center));
        assert_eq!(title.fill, Some(Rgba::rgb(0x1F, 0x49, 0x7D)), "tx2 is dk2 in the theme");
        assert_eq!(title.font_family.as_deref(), Some("Georgia"));
        assert_eq!(title.valign, Some(VAlign::Middle));
        assert_eq!(title.scaling, Some(Scaling::TextDown));
        // The body: the master's place, its bullets, size, and the theme's body font.
        let (words, body) = text_of(&first.objects[2]);
        assert_eq!(words, "First point\nSecond point");
        let frame = first.objects[2].frame.unwrap();
        assert!((f64::from(frame.y) - 1200150.0 * unit).abs() < 0.01);
        assert_eq!(body.list, Some(ListStyle::Bullets));
        assert!((f64::from(body.size.unwrap()) - 28.0 * 12700.0 * unit).abs() < 0.01);
        assert_eq!(body.font_family.as_deref(), Some("Verdana"));
        assert_eq!(body.align, Some(Align::Left));
        assert!(read.check.iter().all(|(n, _)| *n != 1), "a plain slide needs no checking");
    }

    #[test]
    fn a_picture_background_a_text_box_and_a_shape_come_in() {
        let read = read(&sample(), "Sample.pptx", 300, &mut numbered_saver()).unwrap();
        let second = &read.slides[1];
        let kinds: Vec<&str> = second
            .objects
            .iter()
            .map(|o| match &o.kind {
                ObjectKind::Image { .. } => "image",
                ObjectKind::Shape { .. } => "shape",
                ObjectKind::Text { .. } => "text",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["image", "text", "shape"]);
        let ObjectKind::Image { file, fit } = &second.objects[0].kind else { unreachable!() };
        assert_eq!((file.as_str(), *fit), ("picture-1.png", ImageFit::Stretch));
        assert_eq!(second.objects[0].frame, Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 }));
        let (words, style) = text_of(&second.objects[1]);
        assert_eq!(words, "Hello yo");
        assert_eq!((style.weight, style.italic, style.underline), (Some(700), Some(true), Some(true)));
        assert_eq!(style.fill, Some(Rgba::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(style.font_family.as_deref(), Some("Tahoma"));
        assert_eq!(style.align, Some(Align::Right));
        assert_eq!(style.line_height, Some(1.8), "150% of single spacing");
        // "Hello" (7 letters at 20 pt) against " world" (2 at 36 pt): 20 pt has more letters.
        assert!((f64::from(style.size.unwrap()) - 20.0 * 12700.0 * 1920.0 / 9144000.0).abs() < 0.01);
        assert_eq!(style.valign, Some(VAlign::Top));
        let shape = &second.objects[2];
        assert_eq!(shape.fill, Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(shape.stroke.map(|s| s.color), Some(Rgba::rgb(0, 255, 0)));
        assert!(matches!(shape.kind, ObjectKind::Shape { shape: Shape::Rectangle }));
        let why = &read.check.iter().find(|(n, _)| *n == 2).unwrap().1.join("; ");
        assert!(why.contains("mixed sizes"), "{why}");
    }

    #[test]
    fn a_chart_is_drawn_from_its_fallback_picture_or_left_out_and_named() {
        let read = read(&sample(), "Sample.pptx", 300, &mut numbered_saver()).unwrap();
        let third = &read.slides[2];
        assert_eq!(third.objects.len(), 2, "the white background and the chart's picture: {:?}", third.objects);
        assert!(matches!(third.objects[1].kind, ObjectKind::Image { .. }));
        let fourth = &read.slides[3];
        assert_eq!(fourth.objects.len(), 1, "only the background: the chart keeps no picture");
        let why: HashMap<usize, String> = read.check.iter().map(|(n, w)| (*n, w.join("; "))).collect();
        assert!(why[&3].contains("a chart came in as its picture"), "{why:?}");
        assert!(why[&4].contains("a chart couldn't come in"), "{why:?}");
        let note = crate::deck::came_in_note("Sample", read.slides.len(), &read.check);
        assert!(note.starts_with("\u{201c}Sample\u{201d} came in as 5 slides. Check slides 2, 3 and 4: "), "{note}");
        assert!(note.contains("a chart came in as its picture; a chart couldn't come in"), "{note}");
    }

    #[test]
    fn a_slide_to_check_keeps_its_reason_in_its_saved_data_until_it_is_marked() {
        let mut read = read(&sample(), "Sample.pptx", 300, &mut numbered_saver()).unwrap();
        crate::deck::mark_to_check(&mut read.slides, &read.check);
        let checked: Vec<usize> =
            read.slides.iter().enumerate().filter(|(_, s)| s.check.is_some()).map(|(at, _)| at + 1).collect();
        assert_eq!(checked, [2, 3, 4], "the slides the notice names, and no others");
        assert!(read.slides[2].check.as_deref().unwrap().contains("a chart came in as its picture"));
        // Saved and read back, as a restart does: the reason is still there.
        let saved = serde_json::to_string(&read.slides).unwrap();
        let back: Vec<Slide> = serde_json::from_str(&saved).unwrap();
        assert_eq!(back[2].check, read.slides[2].check);
        assert!(!saved.contains("\"check\":null"), "a slide with nothing to check saves no field");
        // A slide saved before there was a check reads back with none.
        let older = serde_json::to_string(&Slide::new(Vec::new())).unwrap();
        assert!(!older.contains("check"));
        assert_eq!(serde_json::from_str::<Slide>(&older).unwrap().check, None);
    }

    /// Testing aid: `SP_WRITE_SAMPLE_PPTX=<file>` writes the five-slide deck these tests read, to
    /// try the app with (slides 2 to 4 are the ones to check).
    #[test]
    #[ignore = "writes a sample deck to try the app with; needs SP_WRITE_SAMPLE_PPTX"]
    fn write_the_sample_deck() {
        let Some(path) = std::env::var_os("SP_WRITE_SAMPLE_PPTX") else { return };
        std::fs::write(path, sample()).unwrap();
    }

    #[test]
    fn hidden_slides_come_in_disabled_and_notes_come_into_the_slide() {
        let read = read(&sample(), "Sample.pptx", 300, &mut numbered_saver()).unwrap();
        let enabled: Vec<bool> = read.slides.iter().map(|s| s.enabled).collect();
        assert_eq!(enabled, [true, true, true, false, true]);
        assert_eq!(read.slides[4].notes.as_deref(), Some("Remember to pray.\nThen the offering."));
        assert_eq!(read.slides[0].notes, None);
    }

    #[test]
    fn a_picture_used_on_two_slides_is_kept_once() {
        let mut kept = Vec::new();
        let mut save = |_: &[u8], ending: &str, _: Option<[f32; 4]>| {
            kept.push(ending.to_owned());
            Ok((format!("only.{ending}"), 2, 2))
        };
        let _ = read(&sample(), "Sample.pptx", 300, &mut save).unwrap();
        assert_eq!(kept.len(), 1, "slides 2 and 3 show the same picture");
    }

    #[test]
    fn a_wide_slide_is_scaled_and_a_four_by_three_one_is_centred() {
        let g = Geometry::new(9144000.0, 6858000.0);
        let whole = g.rect([0.0, 0.0, 9144000.0, 6858000.0]);
        assert!((whole.h - 1080.0).abs() < 0.01 && (whole.w - 1440.0).abs() < 0.01 && (whole.x - 240.0).abs() < 0.01);
        let g = Geometry::new(12192000.0, 6858000.0);
        let whole = g.rect([0.0, 0.0, 12192000.0, 6858000.0]);
        assert!((whole.w - 1920.0).abs() < 0.01 && whole.x.abs() < 0.01);
    }

    #[test]
    fn a_deck_that_declares_a_document_type_or_nests_too_deeply_is_not_read() {
        let entity =
            "<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/hosts\">]><p:sld xmlns:p=\"p\"><p:cSld>&e;</p:cSld></p:sld>";
        assert!(parse(entity).is_none(), "no document type is accepted, so no entity is ever read");
        let deep = format!("{}{}", "<a:x>".repeat(200), "</a:x>".repeat(200));
        assert!(parse(&format!("<r xmlns:a=\"a\">{deep}</r>")).is_none());
        let wide: String = (0..100).map(|n| format!(" a{n}=\"1\"")).collect();
        assert!(parse(&format!("<r{wide}/>")).is_none());
        assert!(parse("<r a=\"1\"><c/></r>").is_some());
    }

    #[test]
    fn a_slide_with_an_entity_is_left_empty_and_named_not_read() {
        let hostile =
            "<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/hosts\">]><p:sld xmlns:p=\"p\"><p:cSld>&e;</p:cSld></p:sld>";
        let mut bytes = zip_without(&sample(), "ppt/slides/slide1.xml");
        let mut parts = unzip_all(&bytes);
        parts.push(("ppt/slides/slide1.xml".to_owned(), hostile.as_bytes().to_vec()));
        bytes = zipped(&parts);
        let read = read(&bytes, "Hostile.pptx", 300, &mut numbered_saver()).unwrap();
        assert!(read.slides[0].objects.is_empty());
        assert!(read.check.iter().any(|(n, why)| *n == 1 && why.join("; ").contains("couldn't be read")));
    }

    fn unzip_all(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..zip.len())
            .map(|at| {
                let mut entry = zip.by_index(at).unwrap();
                let mut read = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut read).unwrap();
                (String::from_utf8_lossy(entry.name_raw()).into_owned(), read)
            })
            .collect()
    }

    fn zip_without(bytes: &[u8], gone: &str) -> Vec<u8> {
        let kept: Vec<_> = unzip_all(bytes).into_iter().filter(|(name, _)| name != gone).collect();
        zipped(&kept)
    }

    #[test]
    fn a_zip_bomb_is_refused_and_so_are_overlapping_entries() {
        // An entry that unpacks to more than the limit allows, from a few kilobytes.
        let mut parts = unzip_all(&sample());
        parts.push(("ppt/slides/bomb.xml".to_owned(), vec![b' '; 70 * 1024 * 1024]));
        let bomb = zipped(&parts);
        assert!(bomb.len() < 200 * 1024, "{} bytes", bomb.len());
        let refused = read(&bomb, "Bomb.pptx", 300, &mut numbered_saver()).unwrap_err();
        assert!(refused.contains("unpacks to more than any presentation"), "{refused}");

        let honest = zipped(&[
            ("ppt/presentation.xml".to_owned(), b"<p/>".to_vec()),
            ("ppt/slides/a.xml".to_owned(), b"<p/>".to_vec()),
        ]);
        assert!(read(&honest, "Honest.pptx", 300, &mut numbered_saver()).is_ok());
        let mut overlapping = honest;
        let u16_at = |bytes: &[u8], at: usize| usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let u32_at = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let end = overlapping.len() - 22;
        let first = u32_at(&overlapping, end + 16);
        let second = first
            + 46
            + u16_at(&overlapping, first + 28)
            + u16_at(&overlapping, first + 30)
            + u16_at(&overlapping, first + 32);
        let offset = overlapping[first + 42..first + 46].to_vec();
        overlapping[second + 42..second + 46].copy_from_slice(&offset);
        let refused = read(&overlapping, "Overlap.pptx", 300, &mut numbered_saver()).unwrap_err();
        assert!(refused.contains("overlap"), "{refused}");
        assert!(
            read(b"not a zip at all", "Text.pptx", 300, &mut numbered_saver()).unwrap_err().contains("isn't a .pptx")
        );
        let no_presentation = zipped(&[("hello.txt".to_owned(), b"hi".to_vec())]);
        assert!(
            read(&no_presentation, "Other.pptx", 300, &mut numbered_saver()).unwrap_err().contains("no presentation")
        );
    }

    #[test]
    fn a_picture_over_the_size_limit_is_left_out_and_its_slide_named() {
        let mut parts = unzip_all(&sample());
        parts.retain(|(name, _)| name != "ppt/media/image1.png");
        parts.push(("ppt/media/image1.png".to_owned(), vec![0; LARGEST_PICTURE + 1]));
        let read = read(&zipped(&parts), "Huge.pptx", 300, &mut numbered_saver()).unwrap();
        assert!(read.slides[1].objects.iter().all(|o| !matches!(o.kind, ObjectKind::Image { .. })));
        let why = &read.check.iter().find(|(n, _)| *n == 2).unwrap().1.join("; ");
        assert!(why.contains("background picture"), "{why}");
    }

    #[test]
    fn names_in_a_zip_are_only_keys_and_never_climb_out_of_the_package() {
        assert_eq!(resolve("ppt/slides", "../media/a.png").as_deref(), Some("ppt/media/a.png"));
        assert_eq!(resolve("ppt/slides", "/ppt/media/a.png").as_deref(), Some("ppt/media/a.png"));
        assert_eq!(resolve("ppt/slides", "../../../etc/passwd"), None);
    }

    #[test]
    fn a_forty_slide_deck_is_read_at_once() {
        let slides: Vec<_> = (0..40)
            .map(|n| {
                (
                    String::new(),
                    slide(
                        "",
                        "",
                        &text_shape(
                            "Title",
                            "<p:ph type=\"title\"/>",
                            "",
                            &format!("<a:p><a:r><a:t>Slide {n}</a:t></a:r></a:p>"),
                        ),
                    ),
                    vec![],
                )
            })
            .collect();
        let bytes = frame(&slides, vec![]);
        let started = std::time::Instant::now();
        let read = read(&bytes, "Forty.pptx", 300, &mut numbered_saver()).unwrap();
        eprintln!("40 slides read in {} ms", started.elapsed().as_millis());
        assert_eq!(read.slides.len(), 40);
        assert!(read.check.is_empty());
    }

    /// Testing aid: `SP_TEST_PPTX=<file>` prints what a deck comes to, slide by slide.
    #[test]
    #[ignore = "reads the deck SP_TEST_PPTX names and prints what it came to"]
    fn describe_a_deck() {
        let Some(file) = std::env::var_os("SP_TEST_PPTX") else { return };
        let bytes = std::fs::read(file).unwrap();
        let started = std::time::Instant::now();
        let read = read(&bytes, "Deck.pptx", 300, &mut numbered_saver()).unwrap();
        eprintln!("{} slides in {} ms", read.slides.len(), started.elapsed().as_millis());
        for (at, slide) in read.slides.iter().enumerate() {
            let parts: Vec<String> = slide
                .objects
                .iter()
                .map(|o| match &o.kind {
                    ObjectKind::Text { text, style } => format!(
                        "text {:?} {}pt {} {:?}",
                        text.chars().take(24).collect::<String>(),
                        style.size.unwrap_or(0.0) / 2.667,
                        style.font_family.clone().unwrap_or_default(),
                        style.fill
                    ),
                    ObjectKind::Image { .. } => {
                        format!("image {:?}", o.frame.map(|f| (f.x as i32, f.y as i32, f.w as i32, f.h as i32)))
                    }
                    ObjectKind::Shape { shape } => format!("shape {shape:?}"),
                    _ => "other".to_owned(),
                })
                .collect();
            let check =
                read.check.iter().find(|(n, _)| *n == at + 1).map(|(_, why)| why.join("; ")).unwrap_or_default();
            eprintln!(
                "{:>2} {}{} | {} || {check}",
                at + 1,
                if slide.enabled { "" } else { "(hidden) " },
                slide.notes.as_deref().map_or("", |_| "(notes) "),
                parts.join(" ; ")
            );
        }
    }
}
