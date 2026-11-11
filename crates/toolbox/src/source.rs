//! The provider trait: what every asset source offers the Toolbox, and the
//! list of sources this build knows.

use crate::http::Http;
use crate::record::{AssetRecord, Kind};
use crate::{SourceError, ambientcg, openverse, polyhaven};

/// Who a source is and what it has. Constant for each source.
#[derive(Debug)]
pub struct SourceInfo {
    /// The key in records and `provider:id`, such as `polyhaven`.
    pub key: &'static str,
    /// The name people see, such as "Poly Haven".
    pub name: &'static str,
    /// The source's site.
    pub home: &'static str,
    /// The credit line shown wherever its results are (Poly Haven's API
    /// terms ask for one).
    pub credit: &'static str,
    /// The kinds it has, in the Toolbox's order.
    pub kinds: &'static [Kind],
    pub licenses: Licenses,
    /// Hosts its files may come from, or `None` for an aggregator whose
    /// files live on many sites (then any https host is allowed).
    pub download_hosts: Option<&'static [&'static str]>,
}

/// The licences a source's assets carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Licenses {
    /// Every asset has this licence code (ambientCG, Poly Haven: CC0).
    All(&'static str),
    /// Each asset states its own.
    PerAsset,
}

impl SourceInfo {
    /// Whether this source has assets of `kind`.
    pub fn has(&self, kind: Kind) -> bool {
        self.kinds.contains(&kind)
    }

    /// Whether `url` is https and on one of the source's download hosts.
    pub fn allows_download_from(&self, url: &str) -> bool {
        let Some(rest) = url.strip_prefix("https://") else {
            return false;
        };
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if host.is_empty() || host.contains(['@', ':']) {
            return false;
        }
        self.download_hosts
            .is_none_or(|hosts| hosts.iter().any(|allowed| *allowed == host))
    }

    /// Whether a query's licence filter can match anything this source
    /// has, without asking it: a source whose assets are all CC0 is ruled
    /// out by a filter that leaves CC0 out.
    pub(crate) fn may_match(&self, query: &Query) -> bool {
        match self.licenses {
            Licenses::All(code) => {
                query.licenses.is_empty()
                    || query
                        .licenses
                        .iter()
                        .any(|wanted| wanted.eq_ignore_ascii_case(code))
            }
            Licenses::PerAsset => true,
        }
    }
}

/// One search.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    /// The words to find; empty lists the source's most popular.
    pub text: String,
    pub kind: Kind,
    /// From 1.
    pub page: u32,
    /// Results a page. Sources may cap it (Openverse: 20).
    pub per_page: u32,
    /// Licence codes to keep (`CC0`, `CC-BY`…); empty keeps any.
    pub licenses: Vec<String>,
}

impl Query {
    /// The first page of 24 results for `text`, any licence.
    pub fn new(text: &str, kind: Kind) -> Self {
        Self {
            text: text.to_owned(),
            kind,
            page: 1,
            per_page: 24,
            licenses: Vec::new(),
        }
    }

    /// The query's offset, from 0.
    pub(crate) fn offset(&self) -> u64 {
        u64::from(self.page.max(1) - 1) * u64::from(self.per_page)
    }
}

/// One page of results.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchPage {
    /// Without variants; ask [`AssetSource::record`] for those.
    pub records: Vec<AssetRecord>,
    /// Whether a next page may have more.
    pub more: bool,
    /// How many match, when the source says.
    pub total: Option<u64>,
}

/// An online catalogue of assets.
///
/// Every call blocks; the editor runs them on its own threads. Failures
/// are [`SourceError`]s whose messages are safe to show.
pub trait AssetSource: Send + Sync {
    /// Who it is and what it has.
    fn info(&self) -> &SourceInfo;

    /// One page of results for `query`. A kind the source doesn't have, or
    /// a licence filter it can't match, is an empty page with no request.
    fn search(&self, http: &dyn Http, query: &Query) -> Result<SearchPage, SourceError>;

    /// The full record for one of its ids, with every variant. An id that
    /// can't be one of the source's is [`SourceError::NotFound`] with no
    /// request.
    fn record(&self, http: &dyn Http, id: &str) -> Result<AssetRecord, SourceError>;
}

/// Every source this build knows, in the order the Toolbox shows them.
pub fn sources() -> [&'static dyn AssetSource; 3] {
    [
        &ambientcg::AMBIENTCG,
        &polyhaven::POLYHAVEN,
        &openverse::OPENVERSE,
    ]
}

/// The source with this key.
pub fn source(key: &str) -> Option<&'static dyn AssetSource> {
    sources()
        .into_iter()
        .find(|source| source.info().key == key)
}

/// The sources that have `kind`, in order.
pub fn sources_for(kind: Kind) -> Vec<&'static dyn AssetSource> {
    sources()
        .into_iter()
        .filter(|source| source.info().has(kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_has_a_unique_key_and_a_credit() {
        let keys: Vec<_> = sources().iter().map(|source| source.info().key).collect();
        assert_eq!(keys, ["ambientcg", "polyhaven", "openverse"]);
        for each in sources() {
            assert!(!each.info().credit.is_empty());
            assert!(source(each.info().key).is_some());
        }
        assert!(source("nowhere").is_none());
    }

    #[test]
    fn sounds_come_from_openverse_and_skies_from_the_other_two() {
        let keys = |kind| -> Vec<_> { sources_for(kind).iter().map(|s| s.info().key).collect() };
        assert_eq!(keys(Kind::Sound), ["openverse"]);
        assert_eq!(keys(Kind::Hdri), ["ambientcg", "polyhaven"]);
    }

    #[test]
    fn downloads_are_only_allowed_from_the_sources_hosts() {
        let poly = polyhaven::POLYHAVEN.info();
        assert!(poly.allows_download_from("https://dl.polyhaven.org/file/a.jpg"));
        assert!(!poly.allows_download_from("http://dl.polyhaven.org/file/a.jpg"));
        assert!(!poly.allows_download_from("https://evil.example/a.jpg"));
        assert!(!poly.allows_download_from("https://dl.polyhaven.org@evil.example/a.jpg"));
        let open = openverse::OPENVERSE.info();
        assert!(open.allows_download_from("https://cdn.freesound.org/a.mp3"));
        assert!(!open.allows_download_from("http://cdn.freesound.org/a.mp3"));
    }

    #[test]
    fn a_cc0_only_source_is_ruled_out_by_a_filter_without_cc0() {
        let mut query = Query::new("brick", Kind::Texture);
        assert!(ambientcg::AMBIENTCG.info().may_match(&query));
        query.licenses = vec!["CC-BY".into()];
        assert!(!ambientcg::AMBIENTCG.info().may_match(&query));
        assert!(openverse::OPENVERSE.info().may_match(&query));
        query.licenses.push("cc0".into());
        assert!(ambientcg::AMBIENTCG.info().may_match(&query));
    }
}
