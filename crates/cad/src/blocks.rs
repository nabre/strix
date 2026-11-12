//! Blocks (docs/cad.md, Blocks): a named set of objects defined once around a base point and
//! inserted as one object, as many times as wanted.
//!
//! A block is **annotative** or not. A plain block keeps its size in the drawing: inserted with
//! factor 1 it measures what it measured when it was made. An annotative block keeps its size
//! **on paper**: it was drawn at the drawing's scale `1:N₀` (the CAD drawing format's), and an
//! insertion at `1:N` is scaled `N / N₀` about its insertion point, so it prints as large as it
//! was drawn. When the drawing's scale changes (picked by hand or fitted to its maximum size)
//! every annotative insertion is scaled again about its insertion point.
//!
//! An insertion is stored expanded: its objects are ordinary entities of the drawing, one group
//! ([`Entity::group`]: picking one picks them all, Explode takes them apart), and an [`Insert`]
//! record names the group, the block and the insertion point. So everything that draws, picks,
//! snaps or exports a drawing works on blocks unchanged; the record only follows the moves and
//! rescales annotative insertions.

use serde::{Deserialize, Serialize};

use crate::geom::Pt;
use crate::model::{Drawing, Entity, Kind, Unit, cut_name};
use crate::modify::Affine;
use crate::{MAX_COORD, MAX_ENTITIES};

/// Most block definitions in one drawing.
pub const MAX_BLOCKS: usize = 256;

/// A block definition: its objects around the base point `(0, 0)`, in `unit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Block {
    pub name: String,
    /// Kept the same size on paper at any scale (see the module's documentation).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub annotative: bool,
    /// The drawing's scale denominator when the block was made (`1:N₀`): the scale its objects
    /// were drawn at.
    pub scale: f64,
    /// The unit its coordinates are in (the drawing's when it was made).
    pub unit: Unit,
    pub entities: Vec<Entity>,
}

impl Default for Block {
    fn default() -> Self {
        Block { name: String::new(), annotative: false, scale: 1.0, unit: Unit::Mm, entities: Vec::new() }
    }
}

/// An insertion of a block: the group its objects form, where it was inserted and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Insert {
    /// The group of the drawing's entities that are this insertion.
    pub group: String,
    /// The block's name.
    pub block: String,
    /// The insertion point, drawing units: the block's base point lands here, and an annotative
    /// insertion is rescaled about it.
    pub at: [f64; 2],
    /// The factor chosen at insertion (then changed by Scale), over the annotative one.
    pub factor: f64,
    /// Turned counter-clockwise by this many degrees.
    #[serde(skip_serializing_if = "is_zero")]
    pub angle: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    /// The drawing's scale denominator its objects are drawn for (an annotative block's; a
    /// plain block's is not used).
    pub drawn_at: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Default for Insert {
    fn default() -> Self {
        Insert { group: String::new(), block: String::new(), at: [0.0, 0.0], factor: 1.0, angle: 0.0, mirror: false, drawn_at: 1.0 }
    }
}

/// What the editor shows about a block before it is inserted.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub name: String,
    pub annotative: bool,
    /// `N₀` of `1:N₀`.
    pub scale: f64,
    pub entities: usize,
    /// Its size in its own unit (width, height), drawing units of the block.
    pub size: (f64, f64),
    /// Its size on paper when it was made, mm.
    pub paper: (f64, f64),
    /// How many times it is inserted.
    pub inserted: usize,
}

/// A scale denominator made safe, as the drawing's (`fallback` when it is not a number).
fn safe_scale(v: f64, fallback: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v.clamp(1e-3, 1e6) } else { fallback }
}

/// A translation by `d`.
fn translation(d: Pt) -> Affine {
    Affine { m: [1.0, 0.0, d.x, 0.0, 1.0, d.y], scale: 1.0, mirror: false }
}

/// `second` after `first`.
fn then(first: &Affine, second: &Affine) -> Affine {
    let [a, b, c, d, e, f] = second.m;
    let [p, q, r, s, t, u] = first.m;
    Affine {
        m: [a * p + b * s, a * q + b * t, a * r + b * u + c, d * p + e * s, d * q + e * t, d * r + e * u + f],
        scale: first.scale * second.scale,
        mirror: first.mirror != second.mirror,
    }
}

impl Drawing {
    /// The block called `name`.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name == name)
    }

    /// The scale an insertion of block `name` takes at this drawing's scale so that it is as
    /// large as it was made: on paper for an annotative block (`N / N₀`), in the drawing for a
    /// plain one (1). A block made in another unit is converted besides. None without such a
    /// block.
    pub fn proposed_factor(&self, name: &str) -> Option<f64> {
        let b = self.block(name)?;
        let annotative = if b.annotative { self.scale / safe_scale(b.scale, 1.0) } else { 1.0 };
        Some(annotative)
    }

    /// What the editor shows about each block.
    pub fn block_infos(&self) -> Vec<BlockInfo> {
        self.blocks
            .iter()
            .map(|b| {
                let tmp = Drawing { entities: b.entities.clone(), unit: b.unit, ..Drawing::default() };
                let all: Vec<usize> = (0..tmp.entities.len()).collect();
                let (lo, hi) = tmp.bounds_of(&all).unwrap_or((Pt::new(0.0, 0.0), Pt::new(0.0, 0.0)));
                let size = (hi.x - lo.x, hi.y - lo.y);
                let k = b.unit.mm() / safe_scale(b.scale, 1.0);
                BlockInfo {
                    name: b.name.clone(),
                    annotative: b.annotative,
                    scale: b.scale,
                    entities: b.entities.len(),
                    size,
                    paper: (size.0 * k, size.1 * k),
                    inserted: self.inserts.iter().filter(|i| i.block == b.name).count(),
                }
            })
            .collect()
    }

    /// The box round the entities at `indices` (their outlines), drawing units.
    pub fn bounds_of(&self, indices: &[usize]) -> Option<(Pt, Pt)> {
        let (mut lo, mut hi) = (Pt::new(f64::INFINITY, f64::INFINITY), Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
        for &i in indices {
            let Some(e) = self.entities.get(i) else { continue };
            for p in self.shape(e).iter().flatten() {
                if p.x.is_finite() && p.y.is_finite() {
                    lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
                    hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
                }
            }
        }
        (lo.x.is_finite() && hi.x.is_finite()).then_some((lo, hi))
    }

    /// The insertion the entity at `i` belongs to.
    pub fn insert_of(&self, i: usize) -> Option<&Insert> {
        let g = self.entities.get(i)?.group.as_deref()?;
        self.inserts.iter().find(|r| r.group == g)
    }

    /// `indices` with what they cannot do without: the hatches' boundaries, the members of their
    /// groups, and the strips and hatches that follow them.
    fn with_dependents(&self, indices: &[usize]) -> Vec<usize> {
        let mut all = self.with_groups(&self.with_boundaries(indices));
        let ids: std::collections::HashSet<&str> = all.iter().filter_map(|&i| self.entities.get(i)).map(|e| e.id.as_str()).collect();
        let follows: Vec<usize> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(_, e)| match &e.kind {
                Kind::Strip { along, .. } => ids.contains(along.as_str()),
                Kind::Hatch { boundary, .. } => !boundary.is_empty() && boundary.iter().all(|b| ids.contains(b.as_str())),
                _ => false,
            })
            .map(|(i, _)| i)
            .collect();
        all.extend(follows);
        let mut seen = std::collections::HashSet::new();
        all.retain(|i| seen.insert(*i));
        all.sort_unstable();
        all
    }

    /// Makes the entities at `indices` a block called `name` with its base point at `base`
    /// (drawing units). An annotative block remembers the drawing's scale as the one it is drawn
    /// at. With `replace`, the objects become an insertion of the new block, where they were;
    /// its indices are returned then.
    pub fn create_block(&mut self, name: &str, indices: &[usize], base: Pt, annotative: bool, replace: bool) -> Result<Vec<usize>, String> {
        let name = cut_name(name.trim());
        if name.is_empty() {
            return Err("Give the block a name".into());
        }
        if self.block(&name).is_some() {
            return Err(format!("There is already a block called {name}"));
        }
        if self.blocks.len() >= MAX_BLOCKS {
            return Err(format!("A drawing holds at most {MAX_BLOCKS} blocks"));
        }
        if !(base.x.is_finite() && base.y.is_finite()) {
            return Err("The base point is not a point".into());
        }
        let members = self.with_dependents(indices);
        if members.is_empty() {
            return Err("Select the objects first".into());
        }
        let held: usize = self.blocks.iter().map(|b| b.entities.len()).sum();
        if held.saturating_add(members.len()) > MAX_ENTITIES {
            return Err(format!("The blocks of a drawing hold at most {MAX_ENTITIES} objects"));
        }
        // The definition: the objects as they are now (their expressions worked out), round the
        // base point, out of any group.
        let mut def = Drawing { unit: self.unit, ..Drawing::default() };
        def.entities = members.iter().filter_map(|&i| self.entities.get(i).cloned()).collect();
        for e in &mut def.entities {
            e.group = None;
        }
        for (n, _) in def.nums_mut() {
            n.expr = None;
        }
        let all: Vec<usize> = (0..def.entities.len()).collect();
        def.translate(&all, Pt::new(-base.x, -base.y));
        def.sanitize();
        self.blocks.push(Block { name: name.clone(), annotative, scale: self.scale, unit: self.unit, entities: def.entities });
        if !replace {
            return Ok(Vec::new());
        }
        self.remove(&members);
        self.insert_block(&name, base, 1.0, 0.0)
    }

    /// Inserts block `name` with its base point at `at`, scaled by `factor` (over an annotative
    /// block's own `N / N₀`) and turned `angle` degrees: its objects added as one group. Returns
    /// their indices.
    pub fn insert_block(&mut self, name: &str, at: Pt, factor: f64, angle: f64) -> Result<Vec<usize>, String> {
        let Some(b) = self.block(name).cloned() else { return Err(format!("There is no block called {name}")) };
        if !(at.x.is_finite() && at.y.is_finite()) || at.x.abs() > MAX_COORD || at.y.abs() > MAX_COORD {
            return Err("The insertion point is not a point of the drawing".into());
        }
        if !(factor.is_finite() && factor > 0.0) {
            return Err("The scale factor is a number more than 0".into());
        }
        let factor = factor.clamp(1e-6, 1e6);
        let angle = if angle.is_finite() { angle % 360.0 } else { 0.0 };
        if b.entities.is_empty() {
            return Err(format!("Block {name} has no objects"));
        }
        if self.entities.len().saturating_add(b.entities.len()) > MAX_ENTITIES {
            return Err(format!("A drawing holds at most {MAX_ENTITIES} entities"));
        }
        // Old id to the new one: a strip follows its path, a hatch its boundaries.
        let olds: Vec<String> = b.entities.iter().map(|e| e.id.clone()).collect();
        let first = self.entities.len();
        let ids = self.add_all(b.entities.clone());
        let renames: std::collections::HashMap<String, String> =
            olds.into_iter().zip(&ids).filter_map(|(old, new)| new.as_ref().map(|n| (old, n.clone()))).collect();
        let made: Vec<usize> = (first..self.entities.len()).collect();
        for &i in &made {
            let rename = |id: &mut String| {
                if let Some(n) = renames.get(id) {
                    n.clone_into(id);
                }
            };
            match self.entities.get_mut(i).map(|e| &mut e.kind) {
                Some(Kind::Strip { along, .. }) => rename(along),
                Some(Kind::Hatch { boundary, .. }) => boundary.iter_mut().for_each(rename),
                _ => {}
            }
        }
        let annotative = if b.annotative { self.scale / safe_scale(b.scale, 1.0) } else { 1.0 };
        let k = b.unit.mm() / self.unit.mm() * factor * annotative;
        let origin = Pt::new(0.0, 0.0);
        let t = then(&then(&Affine::scaling(origin, k), &Affine::rotation(origin, angle)), &translation(at));
        self.transform_only(&made, &t);
        self.group_with(&made, &[]);
        let group = made.first().and_then(|&i| self.entities.get(i)).and_then(|e| e.group.clone()).unwrap_or_default();
        self.inserts.push(Insert { group, block: b.name, at: [at.x, at.y], factor, angle, mirror: false, drawn_at: self.scale });
        self.sanitize();
        Ok(made)
    }

    /// Deletes block `name`'s definition; its insertions stay, as plain groups.
    pub fn delete_block(&mut self, name: &str) -> Result<(), String> {
        let before = self.blocks.len();
        self.blocks.retain(|b| b.name != name);
        if self.blocks.len() == before {
            return Err(format!("There is no block called {name}"));
        }
        self.inserts.retain(|i| i.block != name);
        Ok(())
    }

    /// The insertions whose objects are all among `indices` follow `t` (Move, Rotate, Scale,
    /// Mirror): their insertion point, factor and angle.
    pub(crate) fn inserts_follow(&mut self, indices: &[usize], t: &Affine) {
        if self.inserts.is_empty() {
            return;
        }
        let moved: std::collections::HashSet<usize> = indices.iter().copied().collect();
        for r in &mut self.inserts {
            let mut members =
                self.entities.iter().enumerate().filter(|(_, e)| e.group.as_deref() == Some(r.group.as_str())).map(|(i, _)| i).peekable();
            if members.peek().is_none() || !members.all(|i| moved.contains(&i)) {
                continue;
            }
            let p = t.apply(Pt::new(r.at[0], r.at[1]));
            r.at = [p.x, p.y];
            r.factor *= t.scale;
            r.angle = t.angle(r.angle);
            r.mirror ^= t.mirror;
        }
    }

    /// The insertions copied with their objects (`renames`: each copy's old ids to the new ones,
    /// moved by the offset at the same place).
    pub(crate) fn inserts_copied(&mut self, renames: &[std::collections::HashMap<String, String>], offsets: &[Pt]) {
        if self.inserts.is_empty() {
            return;
        }
        let mut made = Vec::new();
        for (map, d) in renames.iter().zip(offsets) {
            for r in &self.inserts {
                let olds: Vec<&str> = self.entities.iter().filter(|e| e.group.as_deref() == Some(r.group.as_str())).map(|e| e.id.as_str()).collect();
                if olds.is_empty() || !olds.iter().all(|id| map.contains_key(*id)) {
                    continue;
                }
                let group = olds
                    .first()
                    .and_then(|id| map.get(*id))
                    .and_then(|new| self.entities.iter().find(|e| &e.id == new))
                    .and_then(|e| e.group.clone());
                if let Some(group) = group.filter(|g| *g != r.group) {
                    made.push(Insert { group, at: [r.at[0] + d.x, r.at[1] + d.y], ..r.clone() });
                }
            }
        }
        self.inserts.extend(made);
    }

    /// Whether some annotative insertion is drawn for another scale than the drawing's.
    pub(crate) fn annotative_stale(&self) -> bool {
        self.inserts.iter().any(|r| (r.drawn_at - self.scale).abs() > 1e-9 * self.scale && self.block(&r.block).is_some_and(|b| b.annotative))
    }

    /// Every annotative insertion scaled about its insertion point to the drawing's scale.
    pub(crate) fn sync_annotative(&mut self) {
        if !self.annotative_stale() {
            return;
        }
        let scale = self.scale;
        let records: Vec<(usize, String, Pt, f64)> = self
            .inserts
            .iter()
            .enumerate()
            .filter(|(_, r)| self.block(&r.block).is_some_and(|b| b.annotative))
            .map(|(k, r)| (k, r.group.clone(), Pt::new(r.at[0], r.at[1]), scale / safe_scale(r.drawn_at, scale)))
            .collect();
        for (k, group, at, f) in records {
            if (f - 1.0).abs() > 1e-12 {
                let members: Vec<usize> =
                    self.entities.iter().enumerate().filter(|(_, e)| e.group.as_deref() == Some(group.as_str())).map(|(i, _)| i).collect();
                self.transform_only(&members, &Affine::scaling(at, f));
            }
            if let Some(r) = self.inserts.get_mut(k) {
                r.drawn_at = scale;
            }
        }
    }

    /// Blocks and insertions made safe (part of [`Drawing::sanitize`]): names cut, unique and
    /// not empty, at most [`MAX_BLOCKS`] holding at most [`MAX_ENTITIES`] objects together;
    /// insertions only of blocks the drawing has, of groups it has; annotative insertions at
    /// the drawing's scale.
    pub(crate) fn sanitize_blocks(&mut self) {
        if self.blocks.is_empty() && self.inserts.is_empty() {
            return;
        }
        let mut seen = std::collections::HashSet::new();
        let mut budget = MAX_ENTITIES;
        let blocks = std::mem::take(&mut self.blocks);
        for mut b in blocks.into_iter().take(MAX_BLOCKS) {
            b.name = cut_name(b.name.trim());
            if b.name.is_empty() || !seen.insert(b.name.clone()) {
                continue;
            }
            b.scale = safe_scale(b.scale, 1.0);
            b.entities.truncate(budget);
            budget = budget.saturating_sub(b.entities.len());
            let mut def = Drawing { unit: b.unit, entities: std::mem::take(&mut b.entities), ..Drawing::default() };
            for e in &mut def.entities {
                e.group = None;
            }
            def.sanitize();
            b.entities = def.entities;
            self.blocks.push(b);
        }
        let groups: std::collections::HashSet<&str> = self.entities.iter().filter_map(|e| e.group.as_deref()).collect();
        let names: std::collections::HashSet<&str> = self.blocks.iter().map(|b| b.name.as_str()).collect();
        let mut taken = std::collections::HashSet::new();
        let scale = self.scale;
        let inserts = std::mem::take(&mut self.inserts);
        let kept: Vec<Insert> = inserts
            .into_iter()
            .filter(|r| groups.contains(r.group.as_str()) && names.contains(r.block.as_str()) && taken.insert(r.group.clone()))
            .map(|mut r| {
                r.at = r.at.map(|v| if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 });
                r.factor = if r.factor.is_finite() && r.factor > 0.0 { r.factor.clamp(1e-6, 1e6) } else { 1.0 };
                r.angle = if r.angle.is_finite() { r.angle % 360.0 } else { 0.0 };
                r.drawn_at = safe_scale(r.drawn_at, scale);
                r
            })
            .collect();
        self.inserts = kept;
        self.sync_annotative();
    }
}
