/// A grid of heights addressed by global pixel indices, which may have holes.
pub trait Raster {
    /// The height of the pixel at column `x` and row `y` of the grid, or `None` where there is
    /// no data (outside the grid, a tile not loaded, a no-data pixel, a value that is not finite).
    fn value(&self, x: i64, y: i64) -> Option<f64>;

    /// The height at pixel coordinates `(x, y)`, where the centre of pixel `(i, j)` is at
    /// `(i + 0.5, j + 0.5)`: interpolated bilinearly between the four nearest pixel centres.
    /// If some of the four have no data, the value of the nearest one that has is returned
    /// instead; `None` if none of the four has data.
    fn interpolate(&self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (u, v) = (x - 0.5, y - 0.5);
        let (i, j) = (u.floor(), v.floor());
        let (fx, fy) = (u - i, v - j);
        let (i, j) = (i as i64, j as i64);
        let (i1, j1) = (i.saturating_add(1), j.saturating_add(1));
        let corners = [(i, j, (1.0 - fx) * (1.0 - fy)), (i1, j, fx * (1.0 - fy)), (i, j1, (1.0 - fx) * fy), (i1, j1, fx * fy)];
        let values = corners.map(|(ci, cj, weight)| (ci, cj, weight, self.value(ci, cj).filter(|h| h.is_finite())));
        if values.iter().all(|(_, _, _, h)| h.is_some()) {
            return Some(values.iter().map(|(_, _, weight, h)| weight * h.unwrap_or(0.0)).sum());
        }
        let distance = |ci: i64, cj: i64| (ci as f64 - u).powi(2) + (cj as f64 - v).powi(2);
        values.iter().filter_map(|&(ci, cj, _, h)| h.map(|h| (distance(ci, cj), h))).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, h)| h)
    }
}
