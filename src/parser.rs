use std::io::BufRead;

use hashbrown::HashMap;
use quick_xml::{events::Event, Reader};
use thiserror::Error;

use crate::data::{Fude, FudeAttributes, ParsedData, Point, PointRef};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Xml(#[from] quick_xml::Error),
    #[error("Invalid xml: {0}")]
    InvalidData(String),
    #[error("Skipped")]
    SkipAll,
}

pub struct MojxmlParser<R: BufRead> {
    reader: Reader<R>,
    skip_arbitrary_crs: bool,
    buf: Vec<u8>,
    buf2: Vec<u8>,
    points: HashMap<String, Point>,
    segments: HashMap<String, [PointRef; 2]>,
    surfaces: HashMap<String, Vec<Vec<String>>>,
    fudes: HashMap<String, Fude>,
}

impl<R: BufRead> MojxmlParser<R> {
    pub fn new(reader: R) -> Self {
        let mut reader = Reader::from_reader(reader);
        reader.trim_text(true);
        reader.check_end_names(true);
        reader.expand_empty_elements(true);

        Self {
            reader,
            skip_arbitrary_crs: false,
            buf: Vec::new(),
            buf2: Vec::new(),
            points: HashMap::new(),
            segments: HashMap::new(),
            surfaces: HashMap::new(),
            fudes: HashMap::new(),
        }
    }

    pub fn skip_arbitrary_crs(&mut self, skip: bool) {
        self.skip_arbitrary_crs = skip;
    }

    pub fn parse(mut self) -> Result<ParsedData, Error> {
        // Parse the root
        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    if start.name().as_ref() == "地図".as_bytes() {
                        self.parse_chizu()?;
                    } else {
                        return Err(Error::InvalidData(format!(
                            "Unexpected element: {:?}",
                            String::from_utf8_lossy(start.name().as_ref()),
                        )));
                    }
                }
                Event::Text(_) => {
                    return Err(Error::InvalidData(
                        "Unexpected text outside of element".to_string(),
                    ));
                }
                Event::Eof => break,
                _ => {}
            }
        }

        Ok(ParsedData {
            points: self.points,
            segments: self.segments,
            surfaces: self.surfaces,
            fudes: self.fudes,
        })
    }

    fn expect_text(&mut self) -> Result<String, Error> {
        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Text(text) => return Ok(text.unescape()?.into_owned()),
                Event::Start(_) => {
                    return Err(Error::InvalidData(
                        "Expected text but found a start tag".to_string(),
                    ));
                }
                Event::End(_) => {
                    return Err(Error::InvalidData(
                        "Expected text but found an end tag".to_string(),
                    ));
                }
                _ => {}
            }
        }
    }

    fn parse_chizu(&mut self) -> Result<(), Error> {
        // Parse the root <地図> element
        let mut level = 0;

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    match start.local_name().as_ref() {
                        // 空間属性
                        b"\xe7\xa9\xba\xe9\x96\x93\xe5\xb1\x9e\xe6\x80\xa7" => {
                            self.parse_geometric()?;
                        }
                        // 主題属性
                        b"\xe4\xb8\xbb\xe9\xa1\x8c\xe5\xb1\x9e\xe6\x80\xa7" => {
                            self.parse_thematic()?;
                        }
                        // 図郭
                        b"\xe5\x9b\xb3\xe9\x83\xad" => {
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                        }
                        name => {
                            let key = String::from_utf8_lossy(name);
                            // Skip arbitrary coordinate systems
                            if self.skip_arbitrary_crs && key == "座標系" {
                                let value = self.expect_text()?;
                                if value == "任意座標系" {
                                    return Err(Error::SkipAll);
                                }
                            }
                            level += 1;
                        }
                    }
                }
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_geometric(&mut self) -> Result<(), Error> {
        // Parse the <空間属性> element

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    let mut id = None;
                    for attr in start.attributes() {
                        let attr = attr.unwrap();
                        if attr.key.as_ref() == b"id" {
                            id = Some(String::from_utf8_lossy(&attr.value).to_string());
                            break;
                        }
                    }
                    if let Some(id) = id {
                        match start.local_name().as_ref() {
                            b"GM_Point" => {
                                self.parse_point(id)?;
                            }
                            b"GM_Curve" => {
                                self.parse_curve_segment(id)?;
                            }
                            b"GM_Surface" => {
                                self.parse_surface(id)?;
                            }
                            _ => {
                                return Err(Error::InvalidData(format!(
                                    "unexpected element: {:?}",
                                    String::from_utf8_lossy(start.name().as_ref()),
                                )));
                            }
                        }
                    } else {
                        return Err(Error::InvalidData("missing id attribute".to_string()));
                    }
                }
                Event::End(_) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    fn parse_point(&mut self, id: String) -> Result<(), Error> {
        let mut level = 0;
        let mut point = None;

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    level += 1;
                    if start.local_name().as_ref() == b"DirectPosition" {
                        level -= 1;
                        point = Some(self.parse_direct_point()?);
                    }
                }
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        if let Some(point) = point {
                            self.points.insert(id, point);
                        }
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_direct_point(&mut self) -> Result<Point, Error> {
        enum Mode {
            None,
            X,
            Y,
        }
        let mut mode: Mode = Mode::None;
        let mut x: Option<f64> = None;
        let mut y: Option<f64> = None;

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => match (mode, start.local_name().as_ref()) {
                    (Mode::None, b"X") => {
                        mode = Mode::X;
                    }
                    (Mode::None, b"Y") => {
                        mode = Mode::Y;
                    }
                    _ => {
                        return Err(Error::InvalidData(format!(
                            "unexpected element {} in GM_Point",
                            String::from_utf8_lossy(start.local_name().as_ref()),
                        )));
                    }
                },
                Event::Text(text) => {
                    match mode {
                        Mode::X => {
                            x =
                                Some(text.unescape()?.parse().map_err(|_| {
                                    Error::InvalidData("invalid X value".to_string())
                                })?);
                        }
                        Mode::Y => {
                            y =
                                Some(text.unescape()?.parse().map_err(|_| {
                                    Error::InvalidData("invalid Y value".to_string())
                                })?);
                        }
                        Mode::None => {
                            return Err(Error::InvalidData(
                                "unexpected text outside of X/Y element in GM_Point".to_string(),
                            ));
                        }
                    }
                }
                Event::End(_) => match mode {
                    Mode::None => match (x, y) {
                        (Some(x), Some(y)) => {
                            return Ok([x, y]);
                        }
                        _ => {
                            return Err(Error::InvalidData(
                                "missing X or Y value in GM_Point".to_string(),
                            ));
                        }
                    },
                    Mode::X | Mode::Y => {
                        mode = Mode::None;
                    }
                },
                _ => {}
            }
        }
    }

    fn parse_curve_segment(&mut self, id: String) -> Result<(), Error> {
        let mut level = 0;
        let mut num_points = 0;
        let mut points: [PointRef; 2] = [PointRef::Direct([0., 0.]), PointRef::Direct([0., 0.])];

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    level += 1;
                    match start.local_name().as_ref() {
                        b"GM_PointRef.point" => {
                            if num_points >= 2 {
                                return Err(Error::InvalidData(
                                    "Too many points in GM_Curve".to_string(),
                                ));
                            }
                            let mut idref = None;
                            for attr in start.attributes() {
                                let attr = attr.unwrap();
                                if attr.key.as_ref() == b"idref" {
                                    idref = Some(String::from_utf8_lossy(&attr.value).to_string());
                                    break;
                                }
                            }
                            if let Some(idref) = idref {
                                points[num_points] = PointRef::Indirect(idref);
                                num_points += 1;
                            } else {
                                return Err(Error::InvalidData(
                                    "missing idref attribute".to_string(),
                                ));
                            }
                        }
                        b"GM_Position.direct" => {
                            if num_points >= 2 {
                                return Err(Error::InvalidData(
                                    "Too many points in GM_Curve".to_string(),
                                ));
                            }
                            level -= 1;
                            points[num_points] = PointRef::Direct(self.parse_direct_point()?);
                            num_points += 1;
                        }
                        _ => {}
                    }
                }
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        if num_points != 2 {
                            return Err(Error::InvalidData(
                                "Too few points in GM_Curve".to_string(),
                            ));
                        }
                        self.segments.insert(id, points);
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_surface(&mut self, id: String) -> Result<(), Error> {
        let mut level = 0;
        let mut found_exterior = false;
        let mut surface: Vec<Vec<String>> = Vec::with_capacity(1);

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    level += 1;
                    match start.local_name().as_ref() {
                        b"GM_SurfaceBoundary.exterior" => {
                            assert!(!found_exterior, "Multiple exterior rings in GM_Surface");
                            level -= 1;
                            let ring = self.parse_ring()?;
                            surface.insert(0, ring);
                            found_exterior = true;
                        }
                        b"GM_SurfaceBoundary.interior" => {
                            level -= 1;
                            let ring = self.parse_ring()?;
                            surface.push(ring);
                        }
                        _ => {}
                    }
                }
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        if !found_exterior {
                            return Err(Error::InvalidData(
                                "Missing exterior ring in GM_Surface".to_string(),
                            ));
                        }
                        self.surfaces.insert(id, surface);
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_ring(&mut self) -> Result<Vec<String>, Error> {
        let mut level = 0;
        let mut ring: Vec<String> = Vec::with_capacity(4);

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    level += 1;
                    if start.local_name().as_ref() == b"GM_CompositeCurve.generator" {
                        for attr in start.attributes() {
                            let attr = attr.unwrap();
                            if attr.key.as_ref() == b"idref" {
                                let idref = String::from_utf8_lossy(&attr.value).to_string();
                                ring.push(idref);
                                break;
                            }
                        }
                    }
                }
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        return Ok(ring);
                    }
                }
                _ => {}
            }
        }
    }

    fn parse_thematic(&mut self) -> Result<(), Error> {
        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => {
                    let mut id = None;
                    for attr in start.attributes() {
                        let attr = attr.unwrap();
                        if attr.key.as_ref() == b"id" {
                            id = Some(String::from_utf8_lossy(&attr.value).to_string());
                            break;
                        }
                    }
                    match start.local_name().as_ref() {
                        // <筆>
                        b"\xe7\xad\x86" => {
                            let Some(id) = id else {
                                return Err(Error::InvalidData("missing id attribute".to_string()));
                            };
                            let fude = self.parse_fude()?;
                            match fude.attributes.chiban.as_deref() {
                                Some(s) if s.contains("地区外") || s.contains("別図") => {
                                    // skip
                                }
                                _ => {
                                    self.fudes.insert(id, fude);
                                }
                            };
                        }
                        // <基準点> (skip)
                        b"\xe5\x9f\xba\xe6\xba\x96\xe7\x82\xb9" => {
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                        }
                        // <筆界点> (skip)
                        b"\xe7\xad\x86\xe7\x95\x8c\xe7\x82\xb9" => {
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                        }
                        // <仮行政界線> (skip)
                        b"\xe4\xbb\xae\xe8\xa1\x8c\xe6\x94\xbf\xe7\x95\x8c\xe7\xb7\x9a" => {
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                        }
                        // <筆界線> (skip)
                        b"\xe7\xad\x86\xe7\x95\x8c\xe7\xb7\x9a" => {
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                        }
                        _ => {
                            return Err(Error::InvalidData(format!(
                                "unexpected element: {:?}",
                                String::from_utf8_lossy(start.name().as_ref()),
                            )));
                        }
                    }
                }
                Event::End(_) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    fn parse_fude(&mut self) -> Result<Fude, Error> {
        let mut level = 0;

        let mut attributes = FudeAttributes::default();
        let mut surface_id = None;

        loop {
            match self.reader.read_event_into(&mut self.buf)? {
                Event::Start(start) => match start.local_name().as_ref() {
                    // <形状>
                    b"\xe5\xbd\xa2\xe7\x8a\xb6" => {
                        for attr in start.attributes() {
                            let attr = attr.unwrap();
                            if attr.key.as_ref() == b"idref" {
                                let idref = String::from_utf8_lossy(&attr.value).to_string();
                                surface_id = Some(idref);
                                break;
                            }
                        }
                        level += 1;
                    }
                    // other
                    name => {
                        let key = String::from_utf8_lossy(name).into_owned();
                        if key == "筆界未定構成筆" {
                            // TODO: ?
                            self.reader.read_to_end_into(start.name(), &mut self.buf2)?;
                            continue;
                        }
                        let value = self.expect_text()?;

                        match key.as_ref() {
                            "大字コード" => attributes.oaza_code = Some(value),
                            "丁目コード" => attributes.chome_code = Some(value),
                            "小字コード" => attributes.koaza_code = Some(value),
                            "予備コード" => attributes.yobi_code = Some(value),
                            "大字名" => attributes.oaza = Some(value),
                            "丁目名" => attributes.chome = Some(value),
                            "小字名" => attributes.koaza = Some(value),
                            "予備名" => attributes.yobi = Some(value),
                            "地番" => attributes.chiban = Some(value),
                            "精度区分" => attributes.accuracy_class = Some(value),
                            "座標値種別" => attributes.coord_class = Some(value),
                            _ => {
                                return Err(Error::InvalidData(format!(
                                    "Unexpected attribute: {:?}",
                                    key,
                                )));
                            }
                        }
                        level += 1;
                    }
                },
                Event::End(_) => {
                    level -= 1;
                    if level < 0 {
                        return Ok(Fude {
                            attributes,
                            surface_id: surface_id.ok_or_else(|| {
                                Error::InvalidData("Missing surface id in 筆".to_string())
                            })?,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}
