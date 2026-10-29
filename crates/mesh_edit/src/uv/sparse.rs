//! A sparse least-squares solver for the unwrap (Phase 58): the matrix is
//! kept as rows of (column, value) pairs, and `min |A x − b|` is solved by
//! conjugate gradients on the normal equations (CGLS), which only ever
//! multiplies by `A` and its transpose, so `AᵀA` is never built.

/// A matrix kept by its rows, each a list of (column, value).
#[derive(Clone, Debug, Default)]
pub(crate) struct Rows {
    pub columns: usize,
    rows: Vec<Vec<(usize, f64)>>,
}

impl Rows {
    pub fn new(columns: usize) -> Rows {
        Rows {
            columns,
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<(usize, f64)>) {
        self.rows.push(row);
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// `A x`.
    fn times(&self, x: &[f64]) -> Vec<f64> {
        self.rows
            .iter()
            .map(|row| row.iter().map(|&(column, value)| value * x[column]).sum())
            .collect()
    }

    /// `Aᵀ y`.
    fn transpose_times(&self, y: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; self.columns];
        for (row, &weight) in self.rows.iter().zip(y) {
            for &(column, value) in row {
                out[column] += value * weight;
            }
        }
        out
    }
}

/// The `x` that makes `A x` closest to `b`, starting from `guess`: CGLS,
/// until the residual of the normal equations is `tolerance` of where it
/// started, or after `max_steps` steps.
pub(crate) fn least_squares(
    a: &Rows,
    b: &[f64],
    guess: Vec<f64>,
    tolerance: f64,
    max_steps: usize,
) -> Vec<f64> {
    let mut x = guess;
    let ax = a.times(&x);
    let mut r: Vec<f64> = b.iter().zip(&ax).map(|(b, ax)| b - ax).collect();
    let mut s = a.transpose_times(&r);
    let mut p = s.clone();
    let mut gamma = dot(&s, &s);
    let start = gamma.sqrt();
    if start == 0.0 {
        return x;
    }
    for _ in 0..max_steps {
        let q = a.times(&p);
        let qq = dot(&q, &q);
        if qq <= f64::MIN_POSITIVE {
            break;
        }
        let alpha = gamma / qq;
        for (x, p) in x.iter_mut().zip(&p) {
            *x += alpha * p;
        }
        for (r, q) in r.iter_mut().zip(&q) {
            *r -= alpha * q;
        }
        s = a.transpose_times(&r);
        let next = dot(&s, &s);
        if next.sqrt() <= tolerance * start {
            break;
        }
        let beta = next / gamma;
        for (p, s) in p.iter_mut().zip(&s) {
            *p = s + beta * *p;
        }
        gamma = next;
    }
    x
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_overdetermined_system_is_solved_in_the_least_squares_sense() {
        // x = 1, y = 2, x + y = 3.3: the best fit is x = 1.1, y = 2.1.
        let mut a = Rows::new(2);
        a.push(vec![(0, 1.0)]);
        a.push(vec![(1, 1.0)]);
        a.push(vec![(0, 1.0), (1, 1.0)]);
        let x = least_squares(&a, &[1.0, 2.0, 3.3], vec![0.0; 2], 1e-12, 100);
        assert!((x[0] - 1.1).abs() < 1e-9, "{x:?}");
        assert!((x[1] - 2.1).abs() < 1e-9, "{x:?}");
    }
}
