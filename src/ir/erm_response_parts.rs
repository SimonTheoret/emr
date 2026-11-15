use super::{HeadersIR, StatusCodeIR, UrlIR, VersionIR};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERMResponseParts {
    pub url: UrlIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub status: StatusCodeIR,
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        headers::tests::headers_strategy, status_code::tests::status_code_strategy,
        url::tests::url_strategy, version::tests::version_strategy,
    };

    pub(crate) fn response_parts_strategy() -> impl Strategy<Value = ERMResponseParts> {
        (
            url_strategy(),
            version_strategy(),
            headers_strategy(),
            status_code_strategy(),
        )
            .prop_map(|(url, version, headers, status)| ERMResponseParts {
                url,
                version,
                headers,
                status,
            })
    }
}
