// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::PuzzleStats;
use anyhow::{Context, Result};
use chrono::naive::NaiveDate;
use log::{error, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Database {
    records: HashMap<NaiveDate, PuzzleStats>,
    filepath: PathBuf,
}

impl Database {
    /// Create a new database at the given path
    #[must_use]
    pub fn new<T: Into<PathBuf>>(out_path: T) -> Self {
        Self {
            records: HashMap::new(),
            filepath: out_path.into(),
        }
    }

    /// Load a database from file
    pub fn from_file<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref();
        let file =
            File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let records = deserialize_records(file)?;
        Ok(Self {
            records,
            filepath: path.to_path_buf(),
        })
    }

    #[must_use]
    pub fn records(&self) -> Vec<PuzzleStats> {
        self.records.values().copied().collect()
    }

    #[must_use]
    pub fn get(&self, date: NaiveDate) -> Option<PuzzleStats> {
        self.records.get(&date).copied()
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        self.records.contains_key(&date)
    }

    /// Add record to database. If a record already exists for the given date, it will be
    /// overwritten
    pub fn add(&mut self, puzzle: PuzzleStats) {
        self.records.insert(puzzle.date, puzzle);
    }

    /// Write database to file
    pub fn flush(&self) -> Result<()> {
        let mut writer = csv::Writer::from_path(&self.filepath)?;
        let mut sorted = self.records.values().copied().collect::<Vec<PuzzleStats>>();
        sorted.sort_unstable_by_key(|s| s.date);

        // The csv crate's Writer will add a header row using struct fieldnames by default
        for record in sorted {
            writer.serialize(record)?;
        }
        Ok(())
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            error!("Error flushing database: {}", e);
        }
    }
}

fn deserialize_records<R: Read>(reader: R) -> Result<HashMap<NaiveDate, PuzzleStats>> {
    let reader = csv::Reader::from_reader(reader);
    let mut records = HashMap::new();
    for record in reader.into_deserialize() {
        let record: PuzzleStats = record.with_context(|| "Malformed record")?;
        if records.insert(record.date, record).is_some() {
            warn!("Duplicate record in loaded database for {}", record.date);
        }
    }

    Ok(records)
}
