//! Some items for implementing [Content-Security-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Security-Policy/) headers with [axum](https://crates.io/crates/axum)
#![warn(clippy::complexity)]
#![warn(clippy::cargo)]
#![warn(clippy::perf)]
#![deny(unsafe_code)]
#![allow(clippy::multiple_crate_versions)]

use http::HeaderValue;

use regex::RegexSet;
use std::fmt::Debug;

// inspired by https://riptutorial.com/rust/example/5651/serialize-enum-as-string
macro_rules! enum_str {
    // TODO: I might need to make this a proc macro to allow enum documentation
    ($name:ident { $($variant:ident($str:expr), )* }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            $($variant,)*
        }

        impl From<$name> for String {
            fn from(input: $name) -> String {
                match input {
                    $( $name::$variant => $str.to_string(), )*
                }
            }
        }

        // impl From<String> for $name {
        //     fn from(input: String) -> Self {

        //     }
        // }
        // impl ::serde::Serialize for $name {
        //     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        //         where S: ::serde::Serializer,
        //     {
        //         // Serialize the enum as a string.
        //         serializer.serialize_str(match *self {
        //             $( $name::$variant => $str, )*
        //         })
        //     }
        // }

        // impl<'de> serde::de::Deserialize<'de> for $name {
        //     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        //         where D: serde::Deserializer<'de>,
        //     {
        //         struct Visitor;

        //         impl ::serde::de::Visitor<'_> for Visitor {
        //             type Value = $name;

        //             fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        //                 write!(formatter, "a string for {}", stringify!($name))
        //             }

        //             fn visit_str<E>(self, value: &str) -> Result<$name, E>
        //                 where E: ::serde::de::Error,
        //             {
        //                 match value {
        //                     $( $str => Ok($name::$variant), )*
        //                     _ => Err(E::invalid_value(::serde::de::Unexpected::Other(
        //                         &format!("unknown {} variant: {}", stringify!($name), value)
        //                     ), &self)),
        //                 }
        //             }
        //         }

        //         // Deserialize the enum from a string.
        //         deserializer.deserialize_str(Visitor)
        //     }
        // }
    }
}

// TODO: redo this properly, the macro just makes ergonomics crap
enum_str!(
CspDirectiveType {
    ChildSrc("child-src"),
    ConnectSrc("connect-src"),
    DefaultSrc("default-src"),
    FontSrc("font-src"),
    ImgSrc("img-src"),
    ManifestSrc("manifest-src"),
    MediaSrc("media-src"),
    ObjectSrc("object-src"),
    PrefetchSrc("prefetch-src"),
    ScriptSource("script-src"),
    ScriptSourceElem("script-src-elem"),
    StyleSource("style-src"),
    StyleSourceElem("style-src-elem"),
    WorkerSource("worker-src"),
    BaseUri("base-uri"),
    Sandbox("sandbox"),
    FormAction("form-action"),
    FrameAncestors("frame-ancestors"),
    // Experimental!
    NavigateTo("navigate-to"),
    // Experimental/Deprecated, you should use this AND report-to
    ReportUri("report-uri"),
    // Experimental/Deprecated, you should use this AND report-uri
    ReportTo("report-to"),
    // Experimental!
    RequireTrustedTypesFor("require-trusted-types-for"),
    // Experimental!
    TrustedTypes("trusted-types"),
    UpgradeInsecureRequests("upgrade-insecure-requests"),
});

#[derive(Debug, Clone)]
pub struct CspDirective {
    pub directive_type: CspDirectiveType,
    pub values: Vec<CspValue>,
}

impl CspDirective {
    #[must_use]
    pub fn from(directive_type: CspDirectiveType, values: Vec<CspValue>) -> Self {
        Self {
            directive_type,
            values,
        }
    }

    /// Build a default-src 'self' directive
    pub fn default_self() -> Self {
        Self {
            directive_type: CspDirectiveType::DefaultSrc,
            values: vec![CspValue::SelfSite],
        }
    }
}

impl ToString for CspDirective {
    fn to_string(&self) -> String {
        let mut res = String::new();
        for val in self.values.iter() {
            res.push_str(&format!(" {}", String::from(val.to_owned())));
        }
        res.push(';');
        res
    }
}

impl From<CspDirective> for HeaderValue {
    fn from(input: CspDirective) -> HeaderValue {
        HeaderValue::from_str(&input.to_string()).unwrap()
    }
}

/// Build these to find urls to add headers to
#[derive(Clone, Debug)]
pub struct CspUrlMatcher {
    pub matcher: RegexSet,
    pub directives: Vec<CspDirective>,
}

impl CspUrlMatcher {
    #[must_use]
    pub fn new(matcher: RegexSet) -> Self {
        Self {
            matcher,
            directives: vec![],
        }
    }
    pub fn with_directive(&mut self, directive: CspDirective) -> &mut Self {
        self.directives.push(directive);
        self
    }

    /// Exposes the internal matcher.is_match as a struct method
    pub fn is_match(&self, text: &str) -> bool {
        self.matcher.is_match(text)
    }

    /// build a matcher which will emit `default-src 'self';` for all matches
    pub fn default_all_self() -> Self {
        Self {
            matcher: RegexSet::new([r#".*"#]).unwrap(),
            directives: vec![CspDirective {
                directive_type: CspDirectiveType::DefaultSrc,
                values: vec![CspValue::SelfSite],
            }],
        }
    }

    /// build a matcher which will emit `default-src 'self';` for given matches
    pub fn default_self(matcher: RegexSet) -> Self {
        Self {
            matcher,
            directives: vec![CspDirective {
                directive_type: CspDirectiveType::DefaultSrc,
                values: vec![CspValue::SelfSite],
            }],
        }
    }
}

/// Returns the statement as it should show up in the headers
impl From<CspUrlMatcher> for HeaderValue {
    fn from(input: CspUrlMatcher) -> HeaderValue {
        let mut res = String::new();
        for directive in input.directives {
            res.push_str(&format!(" {} ", String::from(directive.directive_type)));
            for val in directive.values {
                res.push_str(&format!(" {}", String::from(val)));
            }
            res.push(';');
        }
        HeaderValue::from_str(&res).unwrap()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Enum for [CSP source values](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Security-Policy/Sources#sources)
pub enum CspValue {
    None,
    /// Equivalent to 'self' but can't just `Self` in rust
    SelfSite,
    StrictDynamic,
    ReportSample,

    UnsafeInline,
    UnsafeEval,
    UnsafeHashes,
    /// Experimental!
    UnsafeAllowRedirects,
    Host {
        value: &'static str,
    },
    SchemeHttps,
    SchemeHttp,
    SchemeData,
    SchemeOther {
        value: &'static str,
    },
    Nonce {
        value: &'static str,
    },
    Sha256 {
        value: &'static str,
    },
    Sha384 {
        value: &'static str,
    },
    Sha512 {
        value: &'static str,
    },
}

impl From<CspValue> for String {
    fn from(input: CspValue) -> String {
        match input {
            CspValue::None => "'none'".to_string(),
            CspValue::SelfSite => "'self'".to_string(),
            CspValue::StrictDynamic => "'strict-dynamic'".to_string(),
            CspValue::ReportSample => "'report-sample'".to_string(),
            CspValue::UnsafeInline => "'unsafe-inline'".to_string(),
            CspValue::UnsafeEval => "'unsafe-eval'".to_string(),
            CspValue::UnsafeHashes => "'unsafe-hashes'".to_string(),
            CspValue::UnsafeAllowRedirects => "'unsafe-allow-redirects'".to_string(),
            CspValue::SchemeHttps => "https:".to_string(),
            CspValue::SchemeHttp => "http:".to_string(),
            CspValue::SchemeData => "data:".to_string(),
            CspValue::Host { value } | CspValue::SchemeOther { value } => value.to_string(),
            CspValue::Nonce { value } => format!("nonce-{value}"),
            CspValue::Sha256 { value } => format!("sha256-{value}"),
            CspValue::Sha384 { value } => format!("sha384-{value}"),
            CspValue::Sha512 { value } => format!("sha512-{value}"),
        }
    }
}
