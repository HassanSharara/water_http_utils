use crate::config::global_config;
use crate::request::{CreatingRequestErrors, HttpPath};

macro_rules! try_increment_index {
    ($index:ident,$len:ident,$last_index_used:ident) => {
          if $index+1 >= $len { return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
          $last_index_used = $index + 1;
    };
     ($index:ident + $num:expr,$len:ident,$last_index_used:ident) => {
          if $index+$num >= $len { return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
          $last_index_used = $index + $num;
    };
}


/// head of http request or the first line
#[derive(Debug)]
pub struct  HttpFirstLine<'buf>{
    pub(crate) version:&'buf str,
    pub(crate) path:HttpPath<'buf>,
    #[cfg(feature = "server")]
    pub (crate) method:&'buf str,

    /// defining first line length
    pub first_line_length:usize,
}


impl <'buf> HttpFirstLine<'buf> {

    #[cfg(feature = "server")]
    #[inline]
    pub (crate) fn from_server(bytes:&'buf[u8]) -> Result<HttpFirstLine<'buf>,CreatingRequestErrors>{
        let mut method = None;
        let mut path = None;
        let mut version = None;
        let mut last_used_index = 0_usize;
        let total_length = bytes.len();
        let global_conf = global_config();
        for (index,byte) in bytes.iter().enumerate() {
            if method.is_none() {
                if index >= global_conf.max_method_size { return CreatingRequestErrors::InvalidHttpFormat.into();}
                match byte {
                    &b' '=>{
                        method = Some(&bytes[..index]);
                        try_increment_index!(index,total_length,last_used_index);
                        continue
                    }
                    _ => {}
                }
            }
            else if path.is_none() {
                if index >= global_conf.max_path_size {return CreatingRequestErrors::DangerousInvalidHttpFormat.into();}
                match byte {
                    &b' '=>{
                        path = Some(&bytes[last_used_index..index]);
                        try_increment_index!(index,total_length,last_used_index);
                        continue
                    }
                    _ => {}
                }
            }
            else if version.is_none(){
                let len = index - last_used_index;
                if len >= global_conf.max_version_size {return CreatingRequestErrors::DangerousInvalidHttpFormat.into();}

                match &byte {
                    &b'\r'=>{
                        let next_index = index + 1 ;
                        if next_index >= total_length {return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
                        if &bytes[next_index] != &b'\n' {continue;}
                        version = Some(&bytes[last_used_index..index]);
                        last_used_index = index + 2;
                        if let Ok(method) = std::str::from_utf8(method.unwrap()) {
                            if let Ok(version) = std::str::from_utf8(version.unwrap()) {
                                return Ok(
                                    HttpFirstLine {
                                        method,
                                        version,
                                        path:HttpPath::new(path.unwrap()),
                                        first_line_length:last_used_index
                                    }
                                    )
                            }
                        }
                        return CreatingRequestErrors::DangerousInvalidHttpFormat.into()
                    }
                    _ => {}
                }
            }
        }
        CreatingRequestErrors::InsufficientDataSoReadMore.into()
    }
}