use hashbrown::HashMap;

pub type Point = [f64; 2];

pub enum PointRef {
    Indirect(String),
    Direct(Point),
}

pub struct Fude {
    pub attributes: FudeAttributes,
    pub surface_id: String,
}

#[derive(Default, Debug)]
pub struct FudeAttributes {
    /// 筆ID
    pub id: String,
    /// 大字コード
    pub oaza_code: Option<String>,
    /// 丁目コード
    pub chome_code: Option<String>,
    /// 小字コード
    pub koaza_code: Option<String>,
    /// 予備コード
    pub yobi_code: Option<String>,
    /// 大字名
    pub oaza: Option<String>,
    /// 丁目名
    pub chome: Option<String>,
    /// 小字名
    pub koaza: Option<String>,
    /// 予備名
    pub yobi: Option<String>,
    /// 地番
    pub chiban: Option<String>,
    /// 筆界未定構成筆
    pub hikkai_mitei: Option<String>,
    /// 精度区分
    pub accuracy_class: Option<String>,
    /// 座標値種別
    pub coord_class: Option<String>,
}

pub struct ParsedData {
    pub points: HashMap<String, Point>,
    pub segments: HashMap<String, [PointRef; 2]>,
    pub surfaces: HashMap<String, Vec<Vec<String>>>,
    pub fudes: HashMap<String, Fude>,
}

impl ParsedData {
    pub fn resolve_surface(&self, surface_id: &str) -> Result<Vec<Vec<Point>>, String> {
        self.surfaces
            .get(surface_id)
            .map(|surface| {
                // rings
                surface
                    .iter()
                    .map(|ring| {
                        // segments
                        ring.iter()
                            .map(|segment_id| match self.segments.get(segment_id) {
                                Some(point_ref) => match point_ref[0] {
                                    PointRef::Direct(point) => Ok(point),
                                    PointRef::Indirect(ref point_id) => self
                                        .points
                                        .get(point_id)
                                        .copied()
                                        .ok_or(format!("Point id={} not found", point_id)),
                                },
                                None => Err(format!("Curve if={} not found", segment_id)),
                            })
                            .collect::<Result<Vec<Point>, _>>()
                    })
                    .collect::<Result<Vec<Vec<Point>>, _>>()
            })
            .ok_or(format!("Surface id={} not found", surface_id))?
    }

    #[cfg(feature = "geo")]
    pub fn resolve_surface_geo(&self, surface_id: &str) -> Result<geo::geometry::Polygon, String> {
        let Some(surface) = self.surfaces.get(surface_id) else {
            return Err(format!("Surface id={} not found", surface_id));
        };
        let exterior = self.ring_to_geo_linestring(&surface[0])?;
        let interiors = surface[1..]
            .iter()
            .map(|ring| self.ring_to_geo_linestring(ring))
            .collect::<Result<Vec<geo::geometry::LineString<f64>>, _>>()?;
        Ok(geo::geometry::Polygon::new(exterior, interiors))
    }

    #[cfg(feature = "geo")]
    fn ring_to_geo_linestring(
        &self,
        ring: &[String],
    ) -> Result<geo::geometry::LineString<f64>, String> {
        ring.iter()
            .map(|segment_id| match self.segments.get(segment_id) {
                Some(point_ref) => match point_ref[0] {
                    PointRef::Direct(point) => Ok(geo::Coord {
                        x: point[0],
                        y: point[1],
                    }),
                    PointRef::Indirect(ref point_id) => self
                        .points
                        .get(point_id)
                        .map(|c| geo::Coord { x: c[1], y: c[0] })
                        .ok_or(format!("Point id={} not found", point_id)),
                },
                None => Err(format!("Curve if={} not found", segment_id)),
            })
            .collect::<Result<geo::geometry::LineString<f64>, _>>()
    }
}
