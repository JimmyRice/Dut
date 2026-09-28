use serde::Serialize;

use dut_core::{
    application::source::Snapshot,
    domain::reference::{Dataset, ReferenceData, SourceFile},
};

use crate::{dto::common::HktTime, http_cache::EntityTag};

/// `GET /api/data`: every dataset and file, with the revision a client
/// compares to decide what to download again.
#[derive(Debug, Serialize)]
pub(crate) struct DataIndexResponse<'a> {
    pub fetched_at: HktTime,
    pub stale: bool,
    pub datasets: Vec<DatasetEntry>,
    pub sources: Vec<SourceEntry<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DatasetEntry {
    pub name: &'static str,
    pub path: String,
    /// The dataset's `ETag`, without `W/` and quotes.
    pub revision: String,
    pub updated_at: Option<HktTime>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SourceEntry<'a> {
    /// The file's name on the MTR's open data portal.
    pub file: &'a str,
    pub path: String,
    /// The file's `ETag`, without `W/` and quotes.
    pub revision: String,
    pub updated_at: Option<HktTime>,
    pub bytes: usize,
}

impl<'a> From<&'a Snapshot<ReferenceData>> for DataIndexResponse<'a> {
    fn from(snapshot: &'a Snapshot<ReferenceData>) -> Self {
        let data = snapshot.value();
        Self {
            fetched_at: HktTime(snapshot.fetched_at()),
            stale: snapshot.freshness().is_stale(),
            datasets: vec![
                DatasetEntry::new("stations", &data.stations),
                DatasetEntry::new("fares", &data.fares),
                DatasetEntry::new("airport-express-fares", &data.airport_express_fares),
                DatasetEntry::new("light-rail", &data.light_rail),
                DatasetEntry::new("light-rail-fares", &data.light_rail_fares),
                DatasetEntry::new("accessibility", &data.accessibility),
            ],
            sources: SourceFile::ALL
                .into_iter()
                .map(|file| {
                    let published = data.files.get(file);
                    SourceEntry {
                        file: file.file_name(),
                        path: format!("/api/data/sources/{}", file.file_name()),
                        revision: EntityTag::file(published.revision()).as_str().to_owned(),
                        updated_at: published.updated_at().map(HktTime),
                        bytes: published.body().len(),
                    }
                })
                .collect(),
        }
    }
}

impl DatasetEntry {
    fn new<T>(name: &'static str, dataset: &Dataset<T>) -> Self {
        Self {
            name,
            path: format!("/api/data/{name}"),
            revision: EntityTag::dataset(dataset.revision()).as_str().to_owned(),
            updated_at: dataset.updated_at().map(HktTime),
        }
    }
}
