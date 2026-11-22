use crate::Coordinate;

/// Where the Sun is in the sky, by NOAA's general solar position calculations
/// (<https://gml.noaa.gov/grad/solcalc/solareqns.PDF>): within about a minute of arc, enough to
/// tell day from night.
pub struct Sun;

impl Sun {
    /// The elevation of the Sun's centre at sunrise and sunset, in degrees: the radius of the
    /// Sun and the refraction of the air (zenith 90.833°).
    pub const HORIZON_DEGREES: f64 = -0.833;
    /// The elevation at the end of civil twilight, in degrees.
    pub const CIVIL_TWILIGHT_DEGREES: f64 = -6.0;

    /// The elevation of the Sun above the horizon at `coordinate`, at `utc_ms` milliseconds since
    /// the epoch, in degrees (without refraction).
    pub fn elevation_deg(coordinate: &Coordinate, utc_ms: i64) -> f64 {
        let days = utc_ms.div_euclid(86_400_000);
        let ms_of_day = utc_ms.rem_euclid(86_400_000) as f64;
        let (year, day_of_year) = Self::year_and_day(days);
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let length = if leap { 366.0 } else { 365.0 };
        let hours = ms_of_day / 3_600_000.0;
        let gamma = 2.0 * std::f64::consts::PI / length * (day_of_year as f64 - 1.0 + (hours - 12.0) / 24.0);
        let eqtime = 229.18 * (0.000075 + 0.001868 * gamma.cos() - 0.032077 * gamma.sin() - 0.014615 * (2.0 * gamma).cos() - 0.040849 * (2.0 * gamma).sin());
        let declination = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin() - 0.006758 * (2.0 * gamma).cos() + 0.000907 * (2.0 * gamma).sin()
            - 0.002697 * (3.0 * gamma).cos()
            + 0.00148 * (3.0 * gamma).sin();
        // True solar time in minutes, in UTC (time zone 0).
        let solar_minutes = hours * 60.0 + eqtime + 4.0 * coordinate.longitude;
        let hour_angle = (solar_minutes / 4.0 - 180.0).to_radians();
        let latitude = coordinate.latitude.to_radians();
        let cos_zenith = latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * hour_angle.cos();
        90.0 - cos_zenith.clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// Whether the Sun has set at `coordinate` at `utc_ms`.
    pub fn is_down(coordinate: &Coordinate, utc_ms: i64) -> bool {
        Self::elevation_deg(coordinate, utc_ms) < Self::HORIZON_DEGREES
    }

    /// The year and the day of the year (1 is 1 January) of a day counted from 1970-01-01, in the
    /// proleptic Gregorian calendar.
    fn year_and_day(days: i64) -> (i64, i64) {
        // Howard Hinnant's civil_from_days.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy_march = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy_march + 2) / 153;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let day = doy_march - (153 * mp + 2) / 5 + 1;
        let year = yoe + era * 400 + i64::from(month <= 2);
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334][(month - 1) as usize];
        (year, before + day + i64::from(leap && month > 2))
    }
}
