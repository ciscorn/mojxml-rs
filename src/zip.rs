use std::io::{Cursor, Read, Seek};

pub struct ZipPackage<R: Read + Seek> {
    zip: zip::ZipArchive<R>,
    inner_zip: Option<zip::ZipArchive<Cursor<Vec<u8>>>>,
    index: usize,
    inner_index: usize,
}

impl<R: Read + Seek> ZipPackage<R> {
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

impl<R: Read + Seek> Iterator for ZipPackage<R> {
    type Item = std::io::Result<(String, Vec<u8>)>;

    fn next(&mut self) -> Option<std::io::Result<(String, Vec<u8>)>> {
        self.next_inner().transpose()
    }
}
