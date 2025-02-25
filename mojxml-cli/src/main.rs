use std::array;
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Instant;

use clap::Parser;

use flatgeobuf::geozero::PropertyProcessor;
use flatgeobuf::{ColumnType, GeometryType};
use geozero::ColumnValue;
use rayon::prelude::*;

#[derive(Parser)]
struct Args {
    /// Input .zip file
    #[arg()]
    input_zip: PathBuf,
    /// Output .fgb file
    #[arg()]
    output_fgb: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let inst = Instant::now();
    let zip = mojxml::zip::ZipPackageParallelIter::new(File::open(args.input_zip)?)?;

    let mut fgb = flatgeobuf::FgbWriter::create_with_options(
        "mojxml",
        GeometryType::Polygon,
        flatgeobuf::FgbWriterOptions {
            crs: flatgeobuf::FgbCrs {
                code: 6668, // JGD2011
                ..Default::default()
            },
            ..Default::default()
        },
    )?;

    fgb.add_column("id", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("大字コード", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("丁目コード", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("小字コード", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("予備コード", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("大字名", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("丁目名", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("小字名", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("予備名", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("地番", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("精度区分", ColumnType::String, |_fbb, _col| {});
    fgb.add_column("座標値種別", ColumnType::String, |_fbb, _col| {});

    let fgb_rw = RwLock::new(fgb);

    let projections: [jprect::etmerc::ExtendedTransverseMercatorProjection; 19] =
        array::from_fn(|i| {
            jprect::JPRZone::from_number(i + 1)
                .expect("ok")
                .projection()
        });

    zip.par_bridge().try_for_each(|res| match res {
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e).into()),
        Ok((name, data)) => {
            eprintln!("File: {}", name);

            let mut reader = Cursor::new(data);
            let mut parser = mojxml::parser::MojxmlParser::new(&mut reader, &projections);
            parser.skip_arbitrary_crs(true);

            match parser.parse() {
                Ok(data) => {
                    for (fude_id, fude) in data.fudes.iter() {
                        if let Ok(poly) = data.resolve_surface_geo(&fude.surface_id) {
                            let geom = geo::geometry::Geometry::Polygon(poly);
                            let mut fgb = fgb_rw.write().unwrap();

                            fgb.add_feature_geom(geom, |feat| {
                                feat.property(0, "id", &ColumnValue::String(fude_id))
                                    .unwrap();

                                if let Some(s) = &fude.attributes.oaza_code {
                                    feat.property(1, "大字コード", &ColumnValue::String(s))
                                        .unwrap();
                                }
                                if let Some(s) = &fude.attributes.chome_code {
                                    feat.property(2, "丁目コード", &ColumnValue::String(s))
                                        .unwrap();
                                }
                                if let Some(s) = &fude.attributes.koaza_code {
                                    feat.property(3, "小字コード", &ColumnValue::String(s))
                                        .unwrap();
                                }
                                if let Some(s) = &fude.attributes.yobi_code {
                                    feat.property(4, "予備コード", &ColumnValue::String(s))
                                        .unwrap();
                                }

                                if let Some(s) = &fude.attributes.oaza {
                                    feat.property(5, "大字名", &ColumnValue::String(s)).unwrap();
                                }
                                if let Some(s) = &fude.attributes.chome {
                                    feat.property(6, "丁目名", &ColumnValue::String(s)).unwrap();
                                }
                                if let Some(s) = &fude.attributes.koaza {
                                    feat.property(7, "小字名", &ColumnValue::String(s)).unwrap();
                                }
                                if let Some(s) = &fude.attributes.yobi {
                                    feat.property(8, "予備名", &ColumnValue::String(s)).unwrap();
                                }

                                if let Some(s) = &fude.attributes.chiban {
                                    feat.property(9, "地番", &ColumnValue::String(s)).unwrap();
                                }
                                if let Some(s) = &fude.attributes.accuracy_class {
                                    feat.property(10, "精度区分", &ColumnValue::String(s))
                                        .unwrap();
                                }
                                if let Some(s) = &fude.attributes.coord_class {
                                    feat.property(11, "座標値種別", &ColumnValue::String(s))
                                        .unwrap();
                                }
                                // if let Some(s) = &fude.attributes.hikkai_mitei {
                                //     feat.property(12, "筆界未定構成筆", &ColumnValue::String(s)).unwrap();
                                // }
                            })
                            .unwrap();
                        }
                    }
                    Ok(())
                }
                Err(mojxml::parser::Error::SkipAll) => Ok(()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    Err(e)
                }
            }
        }
    })?;

    // Write .fgb file
    eprintln!("Writing .fgb file...");
    let fgb = fgb_rw.into_inner().unwrap();
    let file = std::fs::File::create(args.output_fgb)?;
    fgb.write(file)?;

    eprintln!("Elapsed time: {:?}", inst.elapsed());
    Ok(())
}
