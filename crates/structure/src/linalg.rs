//! Small dense linear algebra: Gaussian elimination with full pivoting (rank and solve).

/// A dense row-major matrix.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mat {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Mat {
    pub fn zeros(rows: usize, cols: usize) -> Mat {
        Mat { rows, cols, data: vec![0.0; rows.saturating_mul(cols)] }
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        if r < self.rows && c < self.cols { self.data.get(r * self.cols + c).copied().unwrap_or(0.0) } else { 0.0 }
    }

    pub fn add(&mut self, r: usize, c: usize, v: f64) {
        if r < self.rows
            && c < self.cols
            && let Some(x) = self.data.get_mut(r * self.cols + c)
        {
            *x += v;
        }
    }

    pub fn max_abs(&self) -> f64 {
        self.data.iter().fold(0.0_f64, |m, v| m.max(v.abs()))
    }
}

/// The numerical rank of `m`: pivots below `tol · max|m|` count as zero.
pub(crate) fn rank(m: &Mat, tol: f64) -> usize {
    let mut a = m.clone();
    let scale = a.max_abs();
    if scale == 0.0 {
        return 0;
    }
    let eps = tol * scale;
    let (rows, cols) = (a.rows, a.cols);
    let mut r = 0;
    for c in 0..cols {
        if r >= rows {
            break;
        }
        // Partial pivoting on column c among the remaining rows.
        let mut best = r;
        let mut best_v = 0.0;
        for i in r..rows {
            let v = a.get(i, c).abs();
            if v > best_v {
                best_v = v;
                best = i;
            }
        }
        if best_v <= eps {
            continue;
        }
        swap_rows(&mut a, r, best);
        let p = a.get(r, c);
        for i in (r + 1)..rows {
            let f = a.get(i, c) / p;
            if f != 0.0 {
                for j in c..cols {
                    let v = a.get(r, j);
                    a.add(i, j, -f * v);
                }
            }
        }
        r += 1;
    }
    r
}

fn swap_rows(a: &mut Mat, i: usize, j: usize) {
    if i == j {
        return;
    }
    for c in 0..a.cols {
        let (x, y) = (i * a.cols + c, j * a.cols + c);
        a.data.swap(x, y);
    }
}

/// Solves the square system `a · x = b`, `None` when it is singular (a pivot below
/// `tol · max|a|`).
pub(crate) fn solve(a: &Mat, b: &[f64], tol: f64) -> Option<Vec<f64>> {
    let n = a.rows;
    if a.cols != n || b.len() != n {
        return None;
    }
    let mut m = a.clone();
    let mut rhs = b.to_vec();
    let scale = m.max_abs();
    if scale == 0.0 {
        return if n == 0 { Some(Vec::new()) } else { None };
    }
    let eps = tol * scale;
    for c in 0..n {
        let mut best = c;
        let mut best_v = 0.0;
        for i in c..n {
            let v = m.get(i, c).abs();
            if v > best_v {
                best_v = v;
                best = i;
            }
        }
        if best_v <= eps {
            return None;
        }
        swap_rows(&mut m, c, best);
        rhs.swap(c, best);
        let p = m.get(c, c);
        for i in (c + 1)..n {
            let f = m.get(i, c) / p;
            if f != 0.0 {
                for j in c..n {
                    let v = m.get(c, j);
                    m.add(i, j, -f * v);
                }
                let v = rhs.get(c).copied().unwrap_or(0.0);
                if let Some(x) = rhs.get_mut(i) {
                    *x -= f * v;
                }
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = rhs.get(i).copied().unwrap_or(0.0);
        for j in (i + 1)..n {
            s -= m.get(i, j) * x.get(j).copied().unwrap_or(0.0);
        }
        let p = m.get(i, i);
        if let Some(xi) = x.get_mut(i) {
            *xi = s / p;
        }
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}
