use std::ptr::addr_of;

static mut CONFIG:HttpHConfigurations  = HttpHConfigurations {
     max_method_size:10,
     max_version_size:20,
     max_path_size:1024 * 3,
     max_headers_size:1024 * 10,
     max_body_size:None
};


pub (crate) fn global_config()->&'static HttpHConfigurations {
    unsafe{addr_of!(CONFIG).as_ref().unwrap()}
}

/// setting new Http Configurations for http operations like forming new http request
/// note: this method is not safe for use while running the server ,so you should call this function in the main fn ( the entry point )
pub unsafe fn set_new_http_configurations(conf:HttpHConfigurations){
    CONFIG = conf
}

/// defining important http parsing configurations
#[derive(Debug)]
pub struct HttpHConfigurations {
    /// max incoming request method size
    pub max_method_size:usize,

    /// max incoming request http version size
    pub max_version_size:usize,
    /// max incoming request path size
    pub max_path_size:usize,
    /// defining max headers size without including the first line because we already prepared configs for that
    pub max_headers_size:usize,
    /// max incoming request body size
    pub max_body_size:Option<usize>
}



impl HttpHConfigurations {

    pub fn default()->Self{
        Self {
            max_method_size:10,
            max_version_size:25,
            max_path_size:1024 * 15,
            max_headers_size:1024 * 10,
            max_body_size:None
        }
    }
}