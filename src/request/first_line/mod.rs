use crate::config::global_config;
use crate::request::{CreatingRequestErrors, HttpPath};

// macro_rules! try_increment_index {
//     ($index:ident,$len:ident,$last_index_used:ident) => {
//           if $index+1 >= $len { return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
//           $last_index_used = $index + 1;
//     };
//      ($index:ident + $num:expr,$len:ident,$last_index_used:ident) => {
//           if $index+$num >= $len { return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
//           $last_index_used = $index + $num;
//     };
// }


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

    // #[cfg(feature = "server")]
    // #[inline]
    // pub (crate) fn from_server(bytes:&'buf[u8]) -> Result<HttpFirstLine<'buf>,CreatingRequestErrors>{
    //     let mut method = None;
    //     let mut path = None;
    //     let mut version = None;
    //     let mut last_used_index = 0_usize;
    //     let total_length = bytes.len();
    //     let global_conf = global_config();
    //     for (index,byte) in bytes.iter().enumerate() {
    //         if method.is_none() {
    //             if index >= global_conf.max_method_size { return CreatingRequestErrors::InvalidHttpFormat.into();}
    //             match byte {
    //                 &b' '=>{
    //                     method = Some(&bytes[..index]);
    //                     try_increment_index!(index,total_length,last_used_index);
    //                     continue
    //                 }
    //                 _ => {}
    //             }
    //         }
    //         else if path.is_none() {
    //             if index >= global_conf.max_path_size {return CreatingRequestErrors::DangerousInvalidHttpFormat.into();}
    //             match byte {
    //                 &b' '=>{
    //                     path = Some(&bytes[last_used_index..index]);
    //                     try_increment_index!(index,total_length,last_used_index);
    //                     continue
    //                 }
    //                 _ => {}
    //             }
    //         }
    //         else if version.is_none(){
    //             let len = index - last_used_index;
    //             if len >= global_conf.max_version_size {return CreatingRequestErrors::DangerousInvalidHttpFormat.into();}
    //
    //             match &byte {
    //                 &b'\r'=>{
    //                     let next_index = index + 1 ;
    //                     if next_index >= total_length {return CreatingRequestErrors::InsufficientDataSoReadMore.into()}
    //                     if &bytes[next_index] != &b'\n' {continue;}
    //                     version = Some(&bytes[last_used_index..index]);
    //                     last_used_index = index + 2;
    //                     if let Ok(method) = std::str::from_utf8(method.unwrap()) {
    //                         if let Ok(version) = std::str::from_utf8(version.unwrap()) {
    //                             return Ok(
    //                                 HttpFirstLine {
    //                                     method,
    //                                     version,
    //                                     path:HttpPath::new(path.unwrap()),
    //                                     first_line_length:last_used_index
    //                                 }
    //                                 )
    //                         }
    //                     }
    //                     return CreatingRequestErrors::DangerousInvalidHttpFormat.into()
    //                 }
    //                 _ => {}
    //             }
    //         }
    //     }
    //     CreatingRequestErrors::InsufficientDataSoReadMore.into()
    // }

    #[cfg(feature = "server")]
    #[inline(always)]
    pub (crate) fn from_server(bytes: &'buf [u8]) -> Result<HttpFirstLine<'buf>, CreatingRequestErrors> {
        let len = bytes.len();
        let base_ptr = bytes.as_ptr();
        let global_conf = global_config();

        // 1. ELITE METHOD SCAN: Zero-Branch Detection
        // Most requests start with "GET " (0x20544547) or "POST " (0x2054534f50)
        if len < 12 { return Err(CreatingRequestErrors::InsufficientDataSoReadMore); }

        let (method_len, method_end_ptr) = unsafe {
            let first_8 =(base_ptr as *const u64).read_unaligned() ;
            // Use bitmask to find the space (0x20) in the first 8 bytes
            if (first_8 & 0xFFFFFF) == 0x20544547 { // "GET "
                (3, base_ptr.add(3))
            } else if (first_8 & 0xFFFFFFFFFF) == 0x2054534f50 { // "POST "
                (4, base_ptr.add(4))
            } else if (first_8 & 0xFFFFFFFFFF) == 0x20545550 { // "PUT "
                (3, base_ptr.add(3))
            } else {
                let m_end = memchr::memchr(b' ', bytes)
                    .ok_or(CreatingRequestErrors::InsufficientDataSoReadMore)?;
                if m_end >= global_conf.max_method_size { return Err(CreatingRequestErrors::InvalidHttpFormat); }
                (m_end, base_ptr.add(m_end))
            }
        };

        let method_slice = unsafe { std::slice::from_raw_parts(base_ptr, method_len) };

        // 2. PATH SCAN
        let path_start_ptr = unsafe { method_end_ptr.add(1) };
        let path_max_search = len - (path_start_ptr as usize - base_ptr as usize);

        let path_len = unsafe {
            let search_slice = std::slice::from_raw_parts(path_start_ptr, path_max_search);
            memchr::memchr(b' ', search_slice)
                .ok_or(CreatingRequestErrors::InsufficientDataSoReadMore)?
        };

        if path_len >= global_conf.max_path_size {
            return Err(CreatingRequestErrors::DangerousInvalidHttpFormat);
        }
        let path_slice = unsafe { std::slice::from_raw_parts(path_start_ptr, path_len) };

        // 3. VERSION SCAN
        let version_start_ptr = unsafe { path_start_ptr.add(path_len + 1) };
        let ver_max_search = len - (version_start_ptr as usize - base_ptr as usize);

        let cr_pos = unsafe {
            let search_slice = std::slice::from_raw_parts(version_start_ptr, ver_max_search);
            memchr::memchr(b'\r', search_slice)
                .ok_or(CreatingRequestErrors::InsufficientDataSoReadMore)?
        };

        if cr_pos >= global_conf.max_version_size {
            return Err(CreatingRequestErrors::DangerousInvalidHttpFormat);
        }

        unsafe {
            let lf_ptr = version_start_ptr.add(cr_pos + 1);
            let end_ptr = base_ptr.add(len);

            // Boundary and \n check
            if lf_ptr >= end_ptr || *lf_ptr != b'\n' {
                return Err(CreatingRequestErrors::InvalidHttpFormat);
            }

            let version_slice = std::slice::from_raw_parts(version_start_ptr, cr_pos);

            Ok(HttpFirstLine {
                method: std::str::from_utf8_unchecked(method_slice),
                version: std::str::from_utf8_unchecked(version_slice),
                path: HttpPath::new(path_slice),
                first_line_length: (lf_ptr as usize - base_ptr as usize) + 1,
            })
        }
    }
}