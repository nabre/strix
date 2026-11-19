//! Analysis: classification by the equilibrium matrix, solution by the stiffness method.
//!
//! Degrees of freedom: two translations per node used by a member, and rotations: one per node
//! for the member ends rigidly joined there (and for a support restraining the rotation or a
//! couple applied to the node), one of its own for each hinged member end. Supports are
//! constraints on these, solved with Lagrange multipliers, whose values are the reactions.

use crate::linalg::{Mat, rank, solve};
use crate::model::{Load, LoadAxes, LoadPer, Structure, SupportKind};

/// Relative axial stiffness of a member with no section given (`EA = RIGID · EI / ℓ²`, `ℓ` the
/// longest member): close enough to an inextensible member, still well conditioned.
const RIGID: f64 = 1e6;

/// How the structure is restrained.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// No members.
    Empty,
    /// The data cannot describe a structure (why).
    Invalid(String),
    /// Restrained just enough: the reactions follow from equilibrium alone.
    Isostatic,
    /// Restrained more than enough, by `degree` redundant constraints.
    Hyperstatic { degree: usize },
    /// It can move: `degree` independent mechanisms (and `redundant` constraints that do not help).
    Labile { degree: usize, redundant: usize },
}

impl Status {
    pub fn is_solved(&self) -> bool {
        matches!(self, Status::Isostatic | Status::Hyperstatic { .. })
    }

    /// In words, for messages and the property pane.
    pub fn describe(&self) -> String {
        match self {
            Status::Empty => "no members".to_string(),
            Status::Invalid(why) => why.clone(),
            Status::Isostatic => "isostatic".to_string(),
            Status::Hyperstatic { degree } => format!("hyperstatic, degree {degree}"),
            Status::Labile { degree, .. } => format!("labile, {degree} degree(s) of freedom"),
        }
    }
}

/// A support's reaction, in the structure's axes (forces on the structure).
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    /// Index of the support in [`Structure::supports`].
    pub support: usize,
    pub node: String,
    pub kind: SupportKind,
    /// The support's angle (degrees).
    pub angle: f64,
    pub fx: f64,
    pub fy: f64,
    pub m: f64,
}

/// A load on a member, in its local axes, positions from its `a` end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LocalLoad {
    Point {
        at: f64,
        px: f64,
        py: f64,
    },
    Couple {
        at: f64,
        c: f64,
    },
    /// Linear from `(px1, py1)` at `from` to `(px2, py2)` at `to`, per unit member length.
    Spread {
        from: f64,
        to: f64,
        px1: f64,
        py1: f64,
        px2: f64,
        py2: f64,
    },
}

/// Internal forces at a section: axial force (tension positive), shear (positive turning the
/// piece clockwise) and bending moment (positive stretching the reference fibres).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Forces {
    pub n: f64,
    pub v: f64,
    pub m: f64,
}

/// One member's results.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberResult {
    pub id: String,
    /// Its `a` and `b` ends (structure units).
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub length: f64,
    /// Forces on the member at its `a` end, local axes: `[x, y, moment]`.
    pub(crate) start: [f64; 3],
    pub(crate) loads: Vec<LocalLoad>,
    /// Both ends hinged and no load along it: it only carries an axial force.
    pub truss: bool,
    /// Below these (force, moment) a value is the solver's round-off: zero.
    pub(crate) tol: (f64, f64),
}

impl MemberResult {
    /// Unit vector along the member.
    pub fn dir(&self) -> (f64, f64) {
        if self.length > 0.0 { ((self.b.0 - self.a.0) / self.length, (self.b.1 - self.a.1) / self.length) } else { (1.0, 0.0) }
    }

    /// The internal forces at `x` from the `a` end; at a concentrated load, `after` takes the
    /// value just past it.
    pub fn forces(&self, x: f64, after: bool) -> Forces {
        let [fx, fy, ma] = self.start;
        let (mut n, mut v, mut m) = (-fx, fy, -ma + fy * x);
        let passed = |at: f64| if after { at <= x } else { at < x };
        for l in &self.loads {
            match *l {
                LocalLoad::Point { at, px, py } if passed(at) => {
                    n -= px;
                    v += py;
                    m += py * (x - at);
                }
                LocalLoad::Couple { at, c } if passed(at) => m -= c,
                LocalLoad::Spread { from, to, px1, py1, px2, py2 } if x > from => {
                    let b = x.min(to);
                    let (ix, _) = linear_integrals(from, to, px1, px2, b, x);
                    let (iy, iym) = linear_integrals(from, to, py1, py2, b, x);
                    n -= ix;
                    v += iy;
                    m += iym;
                }
                _ => {}
            }
        }
        let (ft, mt) = self.tol;
        let zero = |v: f64, t: f64| if v.abs() <= t { 0.0 } else { v };
        Forces { n: zero(n, ft), v: zero(v, ft), m: zero(m, mt) }
    }

    /// Where the diagrams may break or change law: the ends and the loads' positions, sorted.
    pub fn breakpoints(&self) -> Vec<f64> {
        let mut xs = vec![0.0, self.length];
        for l in &self.loads {
            match *l {
                LocalLoad::Point { at, .. } | LocalLoad::Couple { at, .. } => xs.push(at),
                LocalLoad::Spread { from, to, .. } => {
                    xs.push(from);
                    xs.push(to);
                }
            }
        }
        xs.retain(|x| x.is_finite());
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * (1.0 + self.length));
        xs
    }

    /// Is there a concentrated load at `x` (the diagrams jump there)?
    pub fn jumps_at(&self, x: f64) -> bool {
        let tol = 1e-9 * (1.0 + self.length);
        self.loads.iter().any(|l| match *l {
            LocalLoad::Point { at, .. } | LocalLoad::Couple { at, .. } => (at - x).abs() <= tol && at > tol && at < self.length - tol,
            LocalLoad::Spread { .. } => false,
        })
    }
}

/// For `p(s)` linear from `p1` at `from` to `p2` at `to`: `∫ p ds` and `∫ p(s)·(x − s) ds` over
/// `[from, b]`.
fn linear_integrals(from: f64, to: f64, p1: f64, p2: f64, b: f64, x: f64) -> (f64, f64) {
    let span = to - from;
    let beta = if span > 0.0 { (p2 - p1) / span } else { 0.0 };
    let alpha = p1 - beta * from;
    let (a, b) = (from, b.max(from));
    let d1 = b - a;
    let d2 = (b * b - a * a) / 2.0;
    let d3 = (b * b * b - a * a * a) / 3.0;
    (alpha * d1 + beta * d2, alpha * x * d1 - alpha * d2 + beta * x * d2 - beta * d3)
}

/// The analysis of a structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub status: Status,
    pub reactions: Vec<Reaction>,
    pub members: Vec<MemberResult>,
    /// Every member carries an axial force only.
    pub truss: bool,
    /// Problems that did not stop the analysis (a load on a missing member…).
    pub warnings: Vec<String>,
}

impl Analysis {
    pub(crate) fn failed(status: Status, warnings: Vec<String>) -> Analysis {
        Analysis { status, reactions: Vec::new(), members: Vec::new(), truss: false, warnings }
    }
}

/// A member as the solver sees it.
struct Elem {
    na: usize,
    nb: usize,
    length: f64,
    c: f64,
    s: f64,
    /// Global DOF indices: ux_a, uy_a, θ_a, ux_b, uy_b, θ_b.
    dofs: [usize; 6],
    ei: f64,
    ea: f64,
    loads: Vec<LocalLoad>,
    hinged: bool,
}

impl Elem {
    /// The local stiffness matrix (u1, v1, θ1, u2, v2, θ2).
    fn k_local(&self) -> [[f64; 6]; 6] {
        let l = self.length;
        let (a, b) = (self.ea / l, self.ei / (l * l * l));
        [
            [a, 0.0, 0.0, -a, 0.0, 0.0],
            [0.0, 12.0 * b, 6.0 * b * l, 0.0, -12.0 * b, 6.0 * b * l],
            [0.0, 6.0 * b * l, 4.0 * b * l * l, 0.0, -6.0 * b * l, 2.0 * b * l * l],
            [-a, 0.0, 0.0, a, 0.0, 0.0],
            [0.0, -12.0 * b, -6.0 * b * l, 0.0, 12.0 * b, -6.0 * b * l],
            [0.0, 6.0 * b * l, 2.0 * b * l * l, 0.0, -6.0 * b * l, 4.0 * b * l * l],
        ]
    }

    /// Global → local for one end's (x, y, θ).
    fn to_local(&self, g: [f64; 6]) -> [f64; 6] {
        let (c, s) = (self.c, self.s);
        [c * g[0] + s * g[1], -s * g[0] + c * g[1], g[2], c * g[3] + s * g[4], -s * g[3] + c * g[4], g[5]]
    }

    fn to_global(&self, l: [f64; 6]) -> [f64; 6] {
        let (c, s) = (self.c, self.s);
        [c * l[0] - s * l[1], s * l[0] + c * l[1], l[2], c * l[3] - s * l[4], s * l[3] + c * l[4], l[5]]
    }

    /// The nodal loads equivalent to the member's loads (local, as forces on the ends).
    fn equivalent(&self) -> [f64; 6] {
        let l = self.length;
        let mut f = [0.0; 6];
        let point = |at: f64, px: f64, py: f64, c: f64, f: &mut [f64; 6]| {
            let x = (at / l).clamp(0.0, 1.0);
            f[0] += px * (1.0 - x);
            f[3] += px * x;
            f[1] += py * (1.0 - 3.0 * x * x + 2.0 * x * x * x) + c * (-6.0 * x + 6.0 * x * x) / l;
            f[2] += py * l * (x - 2.0 * x * x + x * x * x) + c * (1.0 - 4.0 * x + 3.0 * x * x);
            f[4] += py * (3.0 * x * x - 2.0 * x * x * x) + c * (6.0 * x - 6.0 * x * x) / l;
            f[5] += py * l * (-x * x + x * x * x) + c * (-2.0 * x + 3.0 * x * x);
        };
        for ld in &self.loads {
            match *ld {
                LocalLoad::Point { at, px, py } => point(at, px, py, 0.0, &mut f),
                LocalLoad::Couple { at, c } => point(at, 0.0, 0.0, c, &mut f),
                LocalLoad::Spread { from, to, px1, py1, px2, py2 } => {
                    // Gauss–Legendre, 3 points: exact for a linear load on cubic shapes.
                    const G: [(f64, f64); 3] = [(-0.774_596_669_241_483_4, 5.0 / 9.0), (0.0, 8.0 / 9.0), (0.774_596_669_241_483_4, 5.0 / 9.0)];
                    let half = (to - from) / 2.0;
                    if half <= 0.0 {
                        continue;
                    }
                    for (xi, w) in G {
                        let t = (xi + 1.0) / 2.0;
                        let s = from + half * (xi + 1.0);
                        point(s, (px1 + (px2 - px1) * t) * w * half, (py1 + (py2 - py1) * t) * w * half, 0.0, &mut f);
                    }
                }
            }
        }
        f
    }
}

/// The solver's model: degrees of freedom, elements, nodal loads and constraints.
struct Model {
    /// DOFs allocated (translations of every node, then rotations).
    next: usize,
    used: Vec<bool>,
    node_rot: Vec<Option<usize>>,
    elems: Vec<Elem>,
    forces: Vec<(usize, f64, f64)>,
    couples: Vec<(usize, f64)>,
    /// Per constraint: its support's index and its row over the DOFs.
    constraints: Vec<(usize, Vec<(usize, f64)>)>,
    /// DOF → equation row.
    index: Vec<Option<usize>>,
    eqs: usize,
}

impl Model {
    fn new_dof(&mut self) -> usize {
        self.next += 1;
        self.next - 1
    }

    /// The rotation DOF shared by the rigid ends at node `n`.
    fn node_rotation(&mut self, n: usize) -> usize {
        if let Some(Some(r)) = self.node_rot.get(n) {
            return *r;
        }
        let r = self.new_dof();
        if let Some(slot) = self.node_rot.get_mut(n) {
            *slot = Some(r);
        }
        r
    }

    fn is_used(&self, n: usize) -> bool {
        self.used.get(n).copied().unwrap_or(false)
    }

    fn row(&self, d: usize) -> Option<usize> {
        self.index.get(d).copied().flatten()
    }
}

/// Checks the names and builds the elements and their DOFs.
fn build(st: &Structure) -> Result<Model, Status> {
    for (k, n) in st.nodes.iter().enumerate() {
        if n.id.is_empty() || st.nodes.iter().take(k).any(|o| o.id == n.id) {
            return Err(Status::Invalid(format!("node name “{}” is empty or used twice", n.id)));
        }
    }
    for (k, m) in st.members.iter().enumerate() {
        if m.id.is_empty() || st.members.iter().take(k).any(|o| o.id == m.id) {
            return Err(Status::Invalid(format!("member name “{}” is empty or used twice", m.id)));
        }
    }
    let count = st.nodes.len();
    let mut md = Model {
        next: 2 * count,
        used: vec![false; count],
        node_rot: vec![None; count],
        elems: Vec::with_capacity(st.members.len()),
        forces: Vec::new(),
        couples: Vec::new(),
        constraints: Vec::new(),
        index: Vec::new(),
        eqs: 0,
    };
    let longest = st.members.iter().filter_map(|m| st.length(m)).fold(0.0_f64, f64::max).max(1e-9);
    for m in &st.members {
        let missing = || Status::Invalid(format!("member {} joins a node that does not exist", m.id));
        let (na, nb) = (st.node_index(&m.a).ok_or_else(missing)?, st.node_index(&m.b).ok_or_else(missing)?);
        let (pa, pb) = (st.nodes.get(na).ok_or_else(missing)?, st.nodes.get(nb).ok_or_else(missing)?);
        let (dx, dy) = (pb.x.value - pa.x.value, pb.y.value - pa.y.value);
        let length = dx.hypot(dy);
        if na == nb || length <= 1e-9 * longest.max(1.0) {
            return Err(Status::Invalid(format!("member {} has no length", m.id)));
        }
        for n in [na, nb] {
            if let Some(u) = md.used.get_mut(n) {
                *u = true;
            }
        }
        let (hinge_a, hinge_b) = (m.hinge_a || pa.hinge, m.hinge_b || pb.hinge);
        let ra = if hinge_a { md.new_dof() } else { md.node_rotation(na) };
        let rb = if hinge_b { md.new_dof() } else { md.node_rotation(nb) };
        let ei = match (&m.e, &m.i) {
            (None, None) => 1.0,
            (e, i) => e.as_ref().map_or(1.0, |n| n.value) * i.as_ref().map_or(1.0, |n| n.value),
        };
        if !(ei.is_finite() && ei > 0.0) {
            return Err(Status::Invalid(format!("member {}: E·I must be positive", m.id)));
        }
        let ea = match &m.area {
            Some(a) => m.e.as_ref().map_or(1.0, |n| n.value) * a.value,
            None => RIGID * ei / (longest * longest),
        };
        if !(ea.is_finite() && ea > 0.0) {
            return Err(Status::Invalid(format!("member {}: E·A must be positive", m.id)));
        }
        md.elems.push(Elem {
            na,
            nb,
            length,
            c: dx / length,
            s: dy / length,
            dofs: [2 * na, 2 * na + 1, ra, 2 * nb, 2 * nb + 1, rb],
            ei,
            ea,
            loads: Vec::new(),
            hinged: hinge_a && hinge_b,
        });
    }
    // Rotations restrained by supports.
    for s in &st.supports {
        if matches!(s.kind, SupportKind::Fixed | SupportKind::Slider)
            && let Some(n) = st.node_index(&s.node).filter(|n| md.is_used(*n))
        {
            md.node_rotation(n);
        }
    }
    Ok(md)
}

/// The member of a load, by name.
fn elem_of<'m>(st: &Structure, md: &'m mut Model, id: &str, what: &str, warnings: &mut Vec<String>) -> Option<&'m mut Elem> {
    let k = st.members.iter().position(|m| m.id == id);
    if k.is_none() {
        warnings.push(format!("{what} on member {id}, which does not exist: left out"));
    }
    md.elems.get_mut(k?)
}

/// Puts the loads on the nodes and members.
fn place_loads(st: &Structure, md: &mut Model, warnings: &mut Vec<String>) {
    for l in &st.loads {
        match l {
            Load::Force { node: Some(id), member: None, value, angle, .. } => match st.node_index(id).filter(|n| md.is_used(*n)) {
                Some(n) => {
                    let a = angle.value.to_radians();
                    md.forces.push((n, value.value * a.cos(), value.value * a.sin()));
                }
                None => warnings.push(format!("force on node {id}, which no member uses: left out")),
            },
            Load::Moment { node: Some(id), member: None, value, .. } => match st.node_index(id).filter(|n| md.is_used(*n)) {
                Some(n) => {
                    md.node_rotation(n);
                    md.couples.push((n, value.value));
                }
                None => warnings.push(format!("couple on node {id}, which no member uses: left out")),
            },
            Load::Force { member: Some(id), at, value, angle, .. } => {
                let Some(e) = elem_of(st, md, id, "force", warnings) else { continue };
                let a = angle.value.to_radians();
                let (gx, gy) = (value.value * a.cos(), value.value * a.sin());
                let pos = clamp_pos(at.value, e.length, id, warnings);
                e.loads.push(LocalLoad::Point { at: pos, px: e.c * gx + e.s * gy, py: -e.s * gx + e.c * gy });
            }
            Load::Moment { member: Some(id), at, value, .. } => {
                let Some(e) = elem_of(st, md, id, "couple", warnings) else { continue };
                let pos = clamp_pos(at.value, e.length, id, warnings);
                e.loads.push(LocalLoad::Couple { at: pos, c: value.value });
            }
            Load::Force { .. } | Load::Moment { .. } => warnings.push("a concentrated load needs a node or a member: left out".to_string()),
            Load::Distributed { member, q1, q2, from, to, angle, axes, per } => {
                let Some(e) = elem_of(st, md, member, "distributed load", warnings) else { continue };
                let a = clamp_pos(from.value, e.length, member, warnings);
                let b = clamp_pos(to.as_ref().map_or(e.length, |t| t.value), e.length, member, warnings);
                let (a, b) = if a <= b { (a, b) } else { (b, a) };
                if b - a <= 1e-12 * e.length {
                    warnings.push(format!("distributed load on member {member} has no length: left out"));
                    continue;
                }
                let (q1, q2) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                let th = angle.value.to_radians();
                // The load's direction in local axes.
                let (lx, ly) = match axes {
                    LoadAxes::Local => (th.cos(), th.sin()),
                    LoadAxes::Global => {
                        let (gx, gy) = (th.cos(), th.sin());
                        (e.c * gx + e.s * gy, -e.s * gx + e.c * gy)
                    }
                };
                // Per projection: only the member's length across the load counts.
                let factor = match per {
                    LoadPer::Length => 1.0,
                    LoadPer::Projection => ly.abs(),
                };
                e.loads.push(LocalLoad::Spread {
                    from: a,
                    to: b,
                    px1: q1 * lx * factor,
                    py1: q1 * ly * factor,
                    px2: q2 * lx * factor,
                    py2: q2 * ly * factor,
                });
            }
        }
    }
}

/// The supports' constraints, then the equations: one per DOF present.
fn constrain(st: &Structure, md: &mut Model, warnings: &mut Vec<String>) {
    for (k, s) in st.supports.iter().enumerate() {
        let Some(n) = st.node_index(&s.node).filter(|n| md.is_used(*n)) else {
            warnings.push(format!("support on node {}, which no member uses: left out", s.node));
            continue;
        };
        let phi = s.angle.value.to_radians();
        let (nx, ny) = (-phi.sin(), phi.cos());
        let (ux, uy) = (2 * n, 2 * n + 1);
        match s.kind {
            SupportKind::Fixed | SupportKind::Pin => {
                md.constraints.push((k, vec![(ux, 1.0)]));
                md.constraints.push((k, vec![(uy, 1.0)]));
            }
            SupportKind::Roller | SupportKind::RollerCircle | SupportKind::Link | SupportKind::Slider => {
                md.constraints.push((k, vec![(ux, nx), (uy, ny)]))
            }
        }
        if matches!(s.kind, SupportKind::Fixed | SupportKind::Slider)
            && let Some(r) = md.node_rot.get(n).copied().flatten()
        {
            md.constraints.push((k, vec![(r, 1.0)]));
        }
    }
    let mut present = vec![false; md.next];
    let mut mark = |d: usize| {
        if let Some(p) = present.get_mut(d) {
            *p = true;
        }
    };
    for (n, u) in md.used.iter().enumerate() {
        if *u {
            mark(2 * n);
            mark(2 * n + 1);
        }
    }
    for d in md.node_rot.iter().flatten().copied().chain(md.elems.iter().flat_map(|e| [e.dofs[2], e.dofs[5]])) {
        mark(d);
    }
    let mut k = 0;
    md.index = present
        .iter()
        .map(|p| {
            p.then(|| {
                k += 1;
                k - 1
            })
        })
        .collect();
    md.eqs = k;
}

/// Classifies by the rank of the equilibrium matrix (equations × unknowns: three internal
/// forces per member, one per constraint).
fn classify(md: &Model) -> Status {
    let unknowns = 3 * md.elems.len() + md.constraints.len();
    let mut b = Mat::zeros(md.eqs, unknowns);
    for (k, e) in md.elems.iter().enumerate() {
        let l = e.length;
        // Columns: N, Ma, Mb → forces on the member's ends (local), then global.
        let cols: [[f64; 6]; 3] = [[-1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [0.0, 1.0 / l, 1.0, 0.0, -1.0 / l, 0.0], [0.0, 1.0 / l, 0.0, 0.0, -1.0 / l, 1.0]];
        for (j, col) in cols.iter().enumerate() {
            let g = e.to_global(*col);
            for (d, v) in e.dofs.iter().zip(g) {
                if let Some(r) = md.row(*d) {
                    b.add(r, 3 * k + j, v);
                }
            }
        }
    }
    for (j, (_, row)) in md.constraints.iter().enumerate() {
        for (d, v) in row {
            if let Some(r) = md.row(*d) {
                b.add(r, 3 * md.elems.len() + j, -v);
            }
        }
    }
    let rk = rank(&b, 1e-9);
    if rk < md.eqs {
        Status::Labile { degree: md.eqs - rk, redundant: unknowns.saturating_sub(rk) }
    } else if unknowns > rk {
        Status::Hyperstatic { degree: unknowns - rk }
    } else {
        Status::Isostatic
    }
}

/// Element stiffness in global axes: `Tᵀ k T`.
fn k_global(e: &Elem) -> [[f64; 6]; 6] {
    let kl = e.k_local();
    let mut kg = [[0.0; 6]; 6];
    for j in 0..6 {
        let mut unit = [0.0; 6];
        unit[j] = 1.0;
        let local = e.to_local(unit);
        let mut kcol = [0.0; 6];
        for (out, row) in kcol.iter_mut().zip(kl.iter()) {
            *out = row.iter().zip(local.iter()).map(|(a, b)| a * b).sum();
        }
        for (row, v) in kg.iter_mut().zip(e.to_global(kcol)) {
            row[j] = v;
        }
    }
    kg
}

/// Solves `[K Cᵀ; C 0]·[u; λ] = [F; 0]`: the DOFs' displacements then the multipliers.
fn stiffness(md: &Model) -> Option<Vec<f64>> {
    let eqs = md.eqs;
    let size = eqs + md.constraints.len();
    let mut k = Mat::zeros(size, size);
    let mut f = vec![0.0; size];
    let load = |f: &mut Vec<f64>, d: usize, v: f64| {
        if let Some(x) = md.row(d).and_then(|r| f.get_mut(r)) {
            *x += v;
        }
    };
    for e in &md.elems {
        let kg = k_global(e);
        for (i, di) in e.dofs.iter().enumerate() {
            for (j, dj) in e.dofs.iter().enumerate() {
                if let (Some(r), Some(c)) = (md.row(*di), md.row(*dj)) {
                    k.add(r, c, kg[i][j]);
                }
            }
        }
        for (d, v) in e.dofs.iter().zip(e.to_global(e.equivalent())) {
            load(&mut f, *d, v);
        }
    }
    for &(n, fx, fy) in &md.forces {
        load(&mut f, 2 * n, fx);
        load(&mut f, 2 * n + 1, fy);
    }
    for &(n, c) in &md.couples {
        if let Some(r) = md.node_rot.get(n).copied().flatten() {
            load(&mut f, r, c);
        }
    }
    for (j, (_, row)) in md.constraints.iter().enumerate() {
        for (d, v) in row {
            if let Some(r) = md.row(*d) {
                k.add(r, eqs + j, *v);
                k.add(eqs + j, r, *v);
            }
        }
    }
    solve(&k, &f, 1e-14)
}

/// Reactions `R = −Cᵀλ`, gathered per support.
fn reactions_of(st: &Structure, md: &Model, x: &[f64]) -> Vec<Reaction> {
    let mut reactions: Vec<Reaction> = Vec::new();
    for (j, (sk, row)) in md.constraints.iter().enumerate() {
        let lambda = x.get(md.eqs + j).copied().unwrap_or(0.0);
        let Some(s) = st.supports.get(*sk) else { continue };
        let Some(n) = st.node_index(&s.node) else { continue };
        let pos = match reactions.iter().position(|r| r.support == *sk) {
            Some(p) => p,
            None => {
                reactions.push(Reaction { support: *sk, node: s.node.clone(), kind: s.kind, angle: s.angle.value, fx: 0.0, fy: 0.0, m: 0.0 });
                reactions.len() - 1
            }
        };
        let Some(r) = reactions.get_mut(pos) else { continue };
        for (d, v) in row {
            if *d == 2 * n {
                r.fx -= lambda * v;
            } else if *d == 2 * n + 1 {
                r.fy -= lambda * v;
            } else {
                r.m -= lambda * v;
            }
        }
    }
    reactions
}

/// Member end forces `k·d − equivalent loads` (local, on the member).
fn members_of(st: &Structure, md: &Model, x: &[f64]) -> Vec<MemberResult> {
    let u = |d: usize| md.row(d).and_then(|r| x.get(r)).copied().unwrap_or(0.0);
    let mut members = Vec::with_capacity(md.elems.len());
    for (m, e) in st.members.iter().zip(md.elems.iter()) {
        let d = e.to_local(e.dofs.map(u));
        let kl = e.k_local();
        let eq = e.equivalent();
        let end: Vec<f64> = kl.iter().zip(eq).map(|(row, q)| row.iter().zip(d.iter()).map(|(a, b)| a * b).sum::<f64>() - q).collect();
        let (Some(pa), Some(pb)) = (st.nodes.get(e.na), st.nodes.get(e.nb)) else { continue };
        let mut start = [end.first().copied().unwrap_or(0.0), end.get(1).copied().unwrap_or(0.0), end.get(2).copied().unwrap_or(0.0)];
        // A hinged end carries no moment: clear the solver's round-off.
        if Some(e.dofs[2]) != md.node_rot.get(e.na).copied().flatten() {
            start[2] = 0.0;
        }
        members.push(MemberResult {
            id: m.id.clone(),
            a: (pa.x.value, pa.y.value),
            b: (pb.x.value, pb.y.value),
            length: e.length,
            start,
            loads: e.loads.clone(),
            truss: e.hinged && e.loads.is_empty(),
            tol: (0.0, 0.0),
        });
    }
    // The scale of the forces: the members' ends and their loads.
    let mut fmax = 0.0_f64;
    let mut mmax = 0.0_f64;
    let lmax = members.iter().map(|m| m.length).fold(0.0_f64, f64::max);
    for m in &members {
        for x in m.breakpoints() {
            for after in [false, true] {
                let f = m.forces(x, after);
                fmax = fmax.max(f.n.abs()).max(f.v.abs());
                mmax = mmax.max(f.m.abs());
            }
        }
    }
    let ft = 1e-9 * fmax.max(mmax / lmax.max(1e-9));
    let mt = 1e-9 * mmax.max(fmax * lmax);
    for m in &mut members {
        m.tol = (ft, mt);
    }
    members
}

/// Analyses `st` (its numbers already resolved).
pub fn analyse(st: &Structure) -> Analysis {
    let mut warnings = Vec::new();
    if st.members.is_empty() {
        return Analysis::failed(Status::Empty, warnings);
    }
    let mut md = match build(st) {
        Ok(md) => md,
        Err(status) => return Analysis::failed(status, warnings),
    };
    place_loads(st, &mut md, &mut warnings);
    constrain(st, &mut md, &mut warnings);
    let status = classify(&md);
    let truss = md.elems.iter().all(|e| e.hinged && e.loads.is_empty());
    if !status.is_solved() {
        return Analysis { truss, ..Analysis::failed(status, warnings) };
    }
    let Some(x) = stiffness(&md) else {
        return Analysis::failed(Status::Invalid("the structure could not be solved (its stiffnesses are too far apart)".to_string()), warnings);
    };
    let mut reactions = reactions_of(st, &md, &x);
    let mut members = members_of(st, &md, &x);
    clean(&mut reactions, &mut members);
    Analysis { status, reactions, members, truss, warnings }
}

/// Clears round-off: values far below the largest of their kind become zero.
fn clean(reactions: &mut [Reaction], members: &mut [MemberResult]) {
    let fmax = reactions
        .iter()
        .flat_map(|r| [r.fx.abs(), r.fy.abs()])
        .chain(members.iter().flat_map(|m| [m.start[0].abs(), m.start[1].abs()]))
        .fold(0.0_f64, f64::max);
    let mmax = reactions.iter().map(|r| r.m.abs()).chain(members.iter().map(|m| m.start[2].abs())).fold(0.0_f64, f64::max);
    let lmax = members.iter().map(|m| m.length).fold(0.0_f64, f64::max);
    let ftol = 1e-9 * fmax.max(mmax / lmax.max(1e-9));
    let mtol = 1e-9 * mmax.max(fmax * lmax);
    let tidy = |v: &mut f64, tol: f64| {
        if v.abs() <= tol {
            *v = 0.0;
        }
    };
    for r in reactions.iter_mut() {
        tidy(&mut r.fx, ftol);
        tidy(&mut r.fy, ftol);
        tidy(&mut r.m, mtol);
    }
    for m in members.iter_mut() {
        tidy(&mut m.start[0], ftol);
        tidy(&mut m.start[1], ftol);
        tidy(&mut m.start[2], mtol);
    }
}

fn clamp_pos(at: f64, length: f64, member: &str, warnings: &mut Vec<String>) -> f64 {
    if at < -1e-9 * length || at > length * (1.0 + 1e-9) {
        warnings.push(format!("a load on member {member} lies outside it: moved to its end"));
    }
    at.clamp(0.0, length)
}
