use serde::{Deserialize, Serialize};

/// A point on the surface of the Earth, in degrees: latitude north and longitude east positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinate {
    /// The mean radius of the Earth in metres (IUGG, `R1`).
    pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

    pub const fn new(latitude: f64, longitude: f64) -> Self {
        Self { latitude, longitude }
    }

    /// The coordinate, when both values are finite and within their ranges.
    pub fn checked(latitude: f64, longitude: f64) -> Option<Self> {
        let valid = latitude.is_finite() && longitude.is_finite() && (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude);
        valid.then_some(Self { latitude, longitude })
    }

    /// Whether both values are finite and within their ranges.
    pub fn is_valid(&self) -> bool {
        Self::checked(self.latitude, self.longitude).is_some()
    }

    /// The great-circle distance to `other` in metres (the haversine formula, which stays precise
    /// for short distances).
    pub fn distance_m(&self, other: &Coordinate) -> f64 {
        let (phi1, phi2) = (self.latitude.to_radians(), other.latitude.to_radians());
        let dphi = phi2 - phi1;
        let dlambda = (other.longitude - self.longitude).to_radians();
        let a = (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
        2.0 * Self::EARTH_RADIUS_M * a.sqrt().min(1.0).asin()
    }

    /// The initial bearing towards `other`, in degrees clockwise from north, in `[0, 360)`.
    pub fn bearing_to(&self, other: &Coordinate) -> f64 {
        let (phi1, phi2) = (self.latitude.to_radians(), other.latitude.to_radians());
        let dlambda = (other.longitude - self.longitude).to_radians();
        let y = dlambda.sin() * phi2.cos();
        let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlambda.cos();
        Self::normalize_bearing(y.atan2(x).to_degrees())
    }

    /// The point `distance_m` away along the great circle that leaves at `bearing` degrees.
    pub fn destination(&self, bearing: f64, distance_m: f64) -> Coordinate {
        let delta = distance_m / Self::EARTH_RADIUS_M;
        let theta = bearing.to_radians();
        let phi1 = self.latitude.to_radians();
        let lambda1 = self.longitude.to_radians();
        let phi2 = (phi1.sin() * delta.cos() + phi1.cos() * delta.sin() * theta.cos()).asin();
        let lambda2 = lambda1 + (theta.sin() * delta.sin() * phi1.cos()).atan2(delta.cos() - phi1.sin() * phi2.sin());
        Coordinate { latitude: phi2.to_degrees(), longitude: Self::normalize_longitude(lambda2.to_degrees()) }
    }

    /// The point a fraction `t` (0 to 1) of the way to `other`, interpolated linearly in degrees:
    /// exact enough over the length of one segment of a road.
    pub fn lerp(&self, other: &Coordinate, t: f64) -> Coordinate {
        Coordinate { latitude: self.latitude + (other.latitude - self.latitude) * t, longitude: self.longitude + (other.longitude - self.longitude) * t }
    }

    /// Where `other` lies from this point on the plane tangent here, in metres east and north
    /// (equirectangular, exact enough within a few kilometres): for drawing what is around a
    /// position, as a camera view does.
    pub fn offset_m(&self, other: &Coordinate) -> (f64, f64) {
        let metres_per_degree = Self::EARTH_RADIUS_M.to_radians();
        let mean = ((self.latitude + other.latitude) / 2.0).to_radians();
        let dlon = Self::normalize_longitude(other.longitude - self.longitude);
        (dlon * metres_per_degree * mean.cos(), (other.latitude - self.latitude) * metres_per_degree)
    }

    /// `bearing` in `[0, 360)`.
    pub fn normalize_bearing(bearing: f64) -> f64 {
        let b = bearing % 360.0;
        if b < 0.0 { b + 360.0 } else { b }
    }

    /// The signed difference from bearing `from` to bearing `to`, in `(-180, 180]`: positive
    /// clockwise (to the right).
    pub fn bearing_difference(from: f64, to: f64) -> f64 {
        let d = Self::normalize_bearing(to - from);
        if d > 180.0 { d - 360.0 } else { d }
    }

    /// `longitude` in `[-180, 180)`.
    pub fn normalize_longitude(longitude: f64) -> f64 {
        ((longitude + 540.0) % 360.0) - 180.0
    }
}
