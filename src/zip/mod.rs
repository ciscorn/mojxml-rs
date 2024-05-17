//! Utilities for reading the nested-zip distribution.

mod cloneable_seekable_reader;

use std::io::{Cursor, Read, Seek};

pub struct ZipPackageIter<R: Read + Seek> {
    zip: zip::ZipArchive<R>,
    inner_zip: Option<zip::ZipArchive<Cursor<Vec<u8>>>>,
    index: usize,
    inner_index: usize,
}

impl<R: Read + Seek> ZipPackageIter<R> {
    pub fn new(reader: R) -> std::io::Result<Self> {
        let zip = zip::ZipArchive::new(reader)?;
        Ok(Self {
            zip,
            inner_zip: None,
            index: 0,
            inner_index: 0,
        })
    }

    fn next_inner(&mut self) -> std::io::Result<Option<(String, Vec<u8>)>> {
        loop {
            if let Some(inner_zip) = &mut self.inner_zip {
                if self.inner_index < inner_zip.len() {
                    let mut inner_file = inner_zip.by_index(self.inner_index)?;
                    let mut inner = Cursor::new(Vec::new());
                    std::io::copy(&mut inner_file, &mut inner)?;
                    let name = inner_file.name().to_string();
                    self.inner_index += 1;
                    return Ok(Some((name, inner.into_inner())));
                } else {
                    self.inner_zip = None;
                    self.inner_index = 0;
                    self.index += 1;
                }
            }

            if self.index >= self.zip.len() {
                break;
            }

            let mut inner_file = self.zip.by_index(self.index)?;
            match inner_file.name().rsplit_once('.') {
                Some((_, "zip")) => {
                    let mut inner = Cursor::new(Vec::new());
                    std::io::copy(&mut inner_file, &mut inner)?;
                    inner.rewind()?;
                    self.inner_zip = Some(zip::ZipArchive::new(inner)?);
                }
                Some((_, "xml")) => {
                    let mut inner = Cursor::new(Vec::new());
                    std::io::copy(&mut inner_file, &mut inner)?;
                    self.index += 1;
                    return Ok(Some((inner_file.name().to_string(), inner.into_inner())));
                }
                _ => {
                    self.index += 1;
                }
            }
        }
        Ok(None)
    }
}

impl<R: Read + Seek> Iterator for ZipPackageIter<R> {
    type Item = std::io::Result<(String, Vec<u8>)>;

    fn next(&mut self) -> Option<std::io::Result<(String, Vec<u8>)>> {
        self.next_inner().transpose()
    }
}

#[cfg(feature = "rayon")]
mod parallel {
    use super::cloneable_seekable_reader::CloneableSeekableReader;
    pub use super::cloneable_seekable_reader::HasLength;

    use rayon::iter::{ParallelBridge, ParallelIterator};
    use std::{
        io::{Cursor, Read, Seek},
        sync::mpsc,
    };

    pub struct ZipPackageParallelIter {
        receiver: mpsc::Receiver<zip::result::ZipResult<(String, Vec<u8>)>>,
    }

    impl Iterator for ZipPackageParallelIter {
        type Item = zip::result::ZipResult<(String, Vec<u8>)>;

        fn next(&mut self) -> Option<zip::result::ZipResult<(String, Vec<u8>)>> {
            self.receiver.recv().ok()
        }
    }

    impl ZipPackageParallelIter {
        pub fn new<R: Read + Seek + HasLength + Send + 'static>(
            reader: R,
        ) -> std::io::Result<Self> {
            let clonable_reader = CloneableSeekableReader::new(reader);
            let zip = zip::ZipArchive::new(clonable_reader)?;

            let (sender, receiver) = mpsc::sync_channel(32);

            std::thread::spawn(|| {
                rayon::ThreadPoolBuilder::new()
                    .build()
                    .unwrap()
                    .install(|| {
                        Self::producer(zip, sender);
                    });
            });

            Ok(Self { receiver })
        }

        fn producer<R: Clone + Read + Seek + Send>(
            zip: zip::ZipArchive<R>,
            sender: mpsc::SyncSender<zip::result::ZipResult<(String, Vec<u8>)>>,
        ) {
            fn process(
                name: String,
                inner_data: Vec<u8>,
            ) -> zip::result::ZipResult<Option<(String, Vec<u8>)>> {
                match name.rsplit_once('.') {
                    Some((_, "zip")) => {
                        let mut inner_zip = zip::ZipArchive::new(Cursor::new(inner_data))?;
                        assert_eq!(inner_zip.len(), 1);
                        let mut xml = inner_zip.by_index(0)?;
                        let name = xml.name().to_string();
                        if name.ends_with(".xml") {
                            let mut cursor = Cursor::new(Vec::new());
                            std::io::copy(&mut xml, &mut cursor).unwrap();
                            Ok(Some((name, cursor.into_inner())))
                        } else {
                            Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "inner zip does not contain an xml file",
                            )
                            .into())
                        }
                    }
                    Some((_, "xml")) => Ok(Some((name, inner_data))),
                    _ => Ok(None),
                }
            }

            let _ = (0..zip.len())
                .par_bridge()
                .try_for_each_with(zip, |zip, idx| {
                    let mut inner_file = match zip.by_index(idx) {
                        Err(e) => {
                            if sender.send(Err(e)).is_err() {
                                return Err(());
                            }
                            return Ok(());
                        }
                        Ok(inner_file) => inner_file,
                    };
                    inner_file.size();
                    let filename = inner_file.name().to_string();
                    let mut cursor = Cursor::new(Vec::new());
                    if let Err(e) = std::io::copy(&mut inner_file, &mut cursor) {
                        if sender.send(Err(e.into())).is_err() {
                            return Err(());
                        }
                    };
                    let inner_data = cursor.into_inner();

                    match process(filename, inner_data) {
                        Ok(Some((name, data))) => {
                            if sender.send(Ok((name, data))).is_err() {
                                return Err(());
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            if sender.send(Err(e)).is_err() {
                                return Err(());
                            }
                        }
                    }
                    Ok(())
                });
        }
    }
}

#[cfg(feature = "rayon")]
pub use parallel::*;
