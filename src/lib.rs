extern crate iron;
extern crate unicase;

use unicase::UniCase;
use iron::prelude::*;
use iron::method::Method;
use iron::method::Method::*;
use iron::status;
use iron::headers::{Origin, 
                    AccessControlRequestMethod, 
                    AccessControlRequestHeaders, 
                    AccessControlAllowOrigin,
                    AccessControlAllowHeaders, 
                    AccessControlMaxAge, 
                    AccessControlAllowMethods};
use iron::middleware::{AroundMiddleware, Handler};
use std::collections::HashSet;

// Using case-sensitive match of protocol://host:port
// For a formal definition, see
// https://tools.ietf.org/html/rfc6454#section-4
pub enum AllowedOrigins {
    Any,
    Specific(HashSet<String>),
}

impl AllowedOrigins {
    fn answer_to(&self, origin: String) -> Option<String> {
        match self {
            &AllowedOrigins::Any => Some("*".to_owned()),
            &AllowedOrigins::Specific(ref allowed) => {
                if allowed.contains(&origin) {
                    Some(origin)
                } else {
                    None
                }
            }
        }
    }
}


pub struct CorsMiddleware {
    pub allowed_origins: AllowedOrigins,
    pub allowed_methods: Vec<Method>,
    pub allowed_headers: Vec<UniCase<String>>,
    pub exposed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age_seconds: u32,
}

impl CorsMiddleware {
    pub fn new() -> CorsMiddleware {
        let allowed_methods: Vec<Method> = vec![Get, Put, Post];
        let allowed_headers: Vec<unicase::UniCase<String>> =
            vec![// To allow application/json
                 UniCase("Content-Type".to_owned()),
                 // Set by some js libs
                 UniCase("X-Requested-With".to_owned())];
        let exposed_headers: Vec<String> = Vec::new();
        CorsMiddleware {
            allowed_origins: AllowedOrigins::Any,
            allowed_methods: allowed_methods,
            allowed_headers: allowed_headers,
            exposed_headers: exposed_headers,
            allow_credentials: false,
            max_age_seconds: 60 * 60,
        }
    }

    fn handle(&self, req: &mut Request, handler: &Handler) -> IronResult<Response> {
        // http://stackoverflow.com/questions/14015118/
        // what-is-the-expected-response-to-an-invalid-cors-request
        // http://stackoverflow.com/questions/32331737/
        // how-can-i-identify-a-cors-preflight-request
        if req.method == Options &&
           req.headers.get::<AccessControlRequestMethod>().is_some() {
            self.handle_preflight(req, handler)
        } else {
            self.handle_normal(req, handler)
        }
    }

    fn handle_preflight(&self, req: &mut Request, _: &Handler) -> IronResult<Response> {
        // Successful preflight status code is NoContent
        let mut res = Response::with((status::NoContent));
        
        // - Preflight request
        // - 1.If the Origin header is not present terminate this set of steps. The request is
        // - outside the scope of this specification.
        let maybe_origin = req.headers.get::<Origin>();
        if maybe_origin.is_none() {
            let resp = Response::with((status::BadRequest,
                                       "Preflight request without Origin header"));
            return Ok(resp);
        }
        let origin = maybe_origin.unwrap();
        //
        // - 2.If the value of the Origin header is not a case-sensitive match for any of the
        // - values in list of origins do not set any additional headers and terminate this
        // - set of steps.
        //
        // - Note: Always matching is acceptable since the list of origins can be unbounded.
        //
        // - Note: The Origin header can only contain a single origin as the user agent
        //       will not follow redirects.
        //
        let allowed_origin = self.allowed_origins.answer_to(origin.to_string());
        if allowed_origin.is_none() {
            let resp = Response::with((status::BadRequest,
                                       "Preflight request requesting disallowed origin"));
            return Ok(resp);
        }
        //
        // - 3. Let method be the value as result of parsing the Access-Control-Request-Method
        // - header.
        //
        // - If there is no Access-Control-Request-Method header or if parsing failed, do not
        // - set any additional headers and terminate this set of steps. The request is
        // - outside the scope of this specification.
        
        // We can assume that this header exists, since we already checked that before
        // classifying the request as preflight
        let requested_method = req.headers.get::<AccessControlRequestMethod>().unwrap();
        
        //
        // - 4. Let header field-names be the values as result of parsing the
        // - Access-Control-Request-Headers headers.
        let _maybe_requested_headers = req.headers.get::<AccessControlRequestHeaders>();
        //
        // - If there are no Access-Control-Request-Headers headers let header field-names be
        // - the empty list.
        //
        // - If parsing failed do not set any additional headers and terminate this set of
        // - steps. The request is outside the scope of this specification.
        //
        // - 5.If method is not a case-sensitive match for any of the values in list of
        // -   methods do not set any additional headers and terminate this set of steps.
        //
        if !self.allowed_methods.contains(requested_method) {
            return Ok(Response::with((status::BadRequest,
                                       "Preflight request requesting disallowed method")));
        }
        //
        // - Always matching is acceptable since the list of methods can be unbounded.
        //
        // - 6. If any of the header field-names is not a ASCII case-insensitive match for any
        // - of the values in list of headers do not set any additional headers and terminate
        // - this set of steps.
        //
        // - Always matching is acceptable since the list of headers can be unbounded.
        //
        // - 7. If the resource supports credentials add a single Access-Control-Allow-Origin
        // - header, with the value of the Origin header as value, and add a single
        // - Access-Control-Allow-Credentials header with the case-sensitive string "true" as
        // - value.
        //
        // - Otherwise, add a single Access-Control-Allow-Origin header, with either the
        // - value of the Origin header or the string "*" as value.
        res.headers.set(AccessControlAllowOrigin::Value(format!("{}", origin)));
        //
        // - The string "*" cannot be used for a resource that supports credentials.
        //
        // - 8. Optionally add a single Access-Control-Max-Age header with as value the amount
        // - of seconds the user agent is allowed to cache the result of the request.
        res.headers.set(AccessControlMaxAge(self.max_age_seconds));
        //
        // - 9. If method is a simple method this step may be skipped.
        //
        // - Add one or more Access-Control-Allow-Methods headers consisting of (a subset of)
        // - the list of methods.
        res.headers.set(AccessControlAllowMethods(self.allowed_methods.clone()));
        //
        // - If a method is a simple method it does not need to be listed, but this is not
        // - prohibited.
        //
        // - Since the list of methods can be unbounded, simply returning the method
        // - indicated by Access-Control-Request-Method (if supported) can be enough.
        //
        // - 10.If each of the header field-names is a simple header and none is Content-Type,
        // - this step may be skipped.
        //
        // - Add one or more Access-Control-Allow-Headers headers consisting of (a subset of)
        // - the list of headers.
        res.headers.set(AccessControlAllowHeaders(self.allowed_headers.clone()));
        //
        // - If a header field name is a simple header and is not Content-Type, it is not
        // - required to be listed. Content-Type is to be listed as only a subset of its
        // - values makes it qualify as simple header.
        //
        // - Since the list of headers can be unbounded, simply returning supported headers
        // - from Access-Control-Allow-Headers can be enough.
        Ok(res)
    }

    fn handle_normal(&self, req: &mut Request, handler: &Handler) -> IronResult<Response> {
        // Normal request
        // 1.If the Origin header is not present terminate this set of steps. The request is
        // outside the scope of this specification.
        let has_origin = req.headers.get::<Origin>().is_some();
        if !has_origin {
            return handler.handle(req);
        }
        //
        // 2.If the value of the Origin header is not a case-sensitive match for any of the
        // values in list of origins, do not set any additional headers and terminate this
        // set of steps.
        //
        // Note: Always matching is acceptable since the list of origins can be unbounded.
        //
        // 3. If the resource supports credentials add a single Access-Control-Allow-Origin
        // header, with the value of the Origin header as value, and add a single
        // Access-Control-Allow-Credentials header with the case-sensitive string "true" as
        // value.
        //
        // Otherwise, add a single Access-Control-Allow-Origin header, with either the
        // value of the Origin header or the string "*" as value.
        //
        // Note: The string "*" cannot be used for a resource that supports credentials.
        //
        // 4. If the list of exposed headers is not empty add one or more
        // Access-Control-Expose-Headers headers, with as values the header field names
        // given in the list of exposed headers.
        let result = handler.handle(req);
        match result {
            Ok(mut res) => {
                // And set CORS headers
                let origin = req.headers.get::<Origin>().unwrap();
                res.headers.set(AccessControlAllowOrigin::Value(format!("{}", origin)));
                Ok(res)
            }
            _ => result,
        }
    }
}

impl AroundMiddleware for CorsMiddleware {
    fn around(self, handler: Box<Handler>) -> Box<Handler> {
        Box::new(move |req: &mut Request| self.handle(req, &handler))
    }
}

#[cfg(test)]
mod tests {
    extern crate router;
    extern crate hyper;
    extern crate mount;
    use iron::Listening;
    use self::router::Router;
    use iron::prelude::*;
    use iron::status;
    use self::mount::Mount;
    use self::hyper::Client;
    use self::hyper::header::Headers;
    use std::io::Read;
    use iron::headers::{Origin, AccessControlRequestMethod, AccessControlAllowOrigin,
                        AccessControlAllowHeaders, AccessControlMaxAge, AccessControlAllowMethods};
    use iron::method::Method::*;
    use iron::middleware::Handler;
    use super::{CorsMiddleware, AllowedOrigins};
    use std::str::FromStr;
    use std::collections::HashSet;

    struct AutoServer {
        listening: Listening,
        port: u16,
    }

    impl AutoServer {
        pub fn new() -> AutoServer {
            AutoServer::with_cors(CorsMiddleware::new())
        }

        pub fn with_cors(cors: CorsMiddleware) -> AutoServer {
            let get_handler = |_: &mut Request| Ok(Response::with((status::ImATeapot, "")));
            let put_handler = |_: &mut Request| Ok(Response::with((status::BadRequest, "")));

            let mut router = Router::new();
            router.get("", get_handler, "get_a");
            router.put("", put_handler, "put_a");

            let mut chain = Chain::new(router);
            chain.link_around(cors);
            let mut mount = Mount::new();

            mount.mount("/a", chain);
            AutoServer::with_handler(mount)
        }

        pub fn with_handler<H: Handler>(handler: H) -> AutoServer {
            let l = Iron::new(handler).http(format!("localhost:0")).unwrap();
            let p = l.socket.port();
            AutoServer {
                listening: l,
                port: p,
            }
        }
    }

    impl Drop for AutoServer {
        fn drop(&mut self) {
            // Workaround for https://github.com/hyperium/hyper/issues/338
            self.listening.close().unwrap();
        }
    }

    #[test]
    fn normal_request_possible() {
        let server = AutoServer::new();
        let client = Client::new();
        let res = client.get(&format!("http://127.0.0.1:{}/a", server.port)).send().unwrap();
        assert_eq!(res.status, status::ImATeapot);
    }

        fn to_string(res: &mut hyper::client::Response) -> String {
            let mut s = String::new();
            res.read_to_string(&mut s).unwrap();
            s
        }

    #[test]
    fn preflight_to_nonexistent_route_fails() {
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(AccessControlRequestMethod(Get));
        headers.set(Origin::from_str("http://www.a.com:8080").unwrap());
        let mut res = client.request(Options,
                                     &format!("http://127.0.0.1:{}/b", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::NotFound);
        assert_eq!(to_string(&mut res), "");
    }

    #[test]
    fn preflight_without_origin_is_bad_request() {
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(AccessControlRequestMethod(Get));
        let mut res = client.request(Options,
                                     &format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::BadRequest);
        let mut payload = String::new();
        res.read_to_string(&mut payload).unwrap();
        assert_eq!(payload, "Preflight request without Origin header");
    }

    #[test]
    fn preflight_with_allowed_origin_sets_all_headers() {
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(AccessControlRequestMethod(Get));
        headers.set(Origin::from_str("http://www.a.com:8080").unwrap());
        let res = client.request(Options,
                                 &format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::NoContent);
        let allow_origin = res.headers.get::<AccessControlAllowOrigin>().unwrap();
        assert_eq!(format!("{}", allow_origin), "http://www.a.com:8080");
        let allow_headers = res.headers.get::<AccessControlAllowHeaders>().unwrap();
        assert_eq!(format!("{}", allow_headers),
                   "Content-Type, X-Requested-With");
        let allow_methods = res.headers.get::<AccessControlAllowMethods>().unwrap();
        assert_eq!(format!("{}", allow_methods), "GET, PUT, POST");
        let max_age = res.headers.get::<AccessControlMaxAge>().unwrap();
        assert_eq!(max_age.0, 60 * 60u32);
    }

    #[test]
    fn preflight_with_disallowed_origin_is_error() {
        let mut cors = CorsMiddleware::new();
        let origins: HashSet<String> = vec!["http://www.a.com".to_owned()]
            .into_iter().collect();
        cors.allowed_origins = AllowedOrigins::Specific(origins);
        let server = AutoServer::with_cors(cors);
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(AccessControlRequestMethod(Get));
        headers.set(Origin::from_str("http://www.a.com:8080").unwrap());
        let mut res = client.request(Options,
                                 &format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::BadRequest);
        assert_eq!(to_string(&mut res), "Preflight request requesting disallowed origin");
    }

    #[test]
    fn options_without_method_is_normal_request() {
        // A requestion with options and origin but without
        // method is considers non-preflight
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(Origin::from_str("http://a.com").unwrap());
        let mut res = client.request(Options,
                                     &format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::Ok);
        let mut payload = String::new();
        res.read_to_string(&mut payload).unwrap();
        assert_eq!(payload, "");
    }

    #[test]
    fn preflight_with_disallowed_method_is_error() {
        // A requestion with options and origin but without
        // method is considers non-preflight
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(AccessControlRequestMethod(Patch));
        headers.set(Origin::from_str("http://a.com").unwrap());
        let mut res = client.request(Options,
                                     &format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::BadRequest);
        let mut payload = String::new();
        res.read_to_string(&mut payload).unwrap();
        assert_eq!(payload, "Preflight request requesting disallowed method");
    }

    #[test]
    fn normal_request_allows_origin() {
        let server = AutoServer::new();
        let client = Client::new();
        let mut headers = Headers::new();
        headers.set(Origin::from_str("http://www.a.com:8080").unwrap());
        let res = client.get(&format!("http://127.0.0.1:{}/a", server.port))
            .headers(headers)
            .send()
            .unwrap();
        assert_eq!(res.status, status::ImATeapot);
        let allow_origin = res.headers.get::<AccessControlAllowOrigin>().unwrap();
        assert_eq!(format!("{}", allow_origin), "http://www.a.com:8080");
        assert!(res.headers.get::<AccessControlAllowHeaders>().is_none());
        assert!(res.headers.get::<AccessControlAllowMethods>().is_none());
        assert!(res.headers.get::<AccessControlMaxAge>().is_none());
    }

    #[test]
    fn normal_request_without_origin_is_passthrough() {
        let server = AutoServer::new();
        let client = Client::new();
        let res = client.get(&format!("http://127.0.0.1:{}/a", server.port)).send().unwrap();
        assert_eq!(res.status, status::ImATeapot);
        assert!(res.headers.get::<AccessControlAllowOrigin>().is_none());
        assert!(res.headers.get::<AccessControlAllowHeaders>().is_none());
        assert!(res.headers.get::<AccessControlAllowMethods>().is_none());
        assert!(res.headers.get::<AccessControlMaxAge>().is_none());
    }

}
