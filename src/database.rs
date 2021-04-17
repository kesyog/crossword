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

use crate::api_client::SolvedPuzzleStats;
use anyhow::{Context, Result};
use chrono::{naive::NaiveDate, Datelike, Weekday};
use log::{error, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize, Serialize)]
pub struct PuzzleStats {
    pub date: NaiveDate,
    /// id used to identify a puzzle to NYT server
    /// TODO: consider removing Option wrapper once database has been fully updated with ids
    pub puzzle_id: Option<u32>,
    weekday: Weekday,
    // It would be nice to embed SolvedPuzzleStats here, but serde's flatten attribute doesn't play
    // well with the csv crate
    pub solve_time_secs: Option<u32>,
    opened_unix: Option<u32>,
    solved_unix: Option<u32>,
}

impl PuzzleStats {
    pub fn new(date: NaiveDate, id: u32, solve_stats: Option<SolvedPuzzleStats>) -> Self {
        let weekday = date.weekday();
        Self {
            date,
            puzzle_id: Some(id),
            weekday,
            solve_time_secs: solve_stats.map(|s| s.solve_time),
            opened_unix: solve_stats.and_then(|s| s.opened),
            solved_unix: solve_stats.and_then(|s| s.solved),
        }
    }

    pub fn empty(date: NaiveDate) -> Self {
        let weekday = date.weekday();
        Self {
            date,
            weekday,
            puzzle_id: None,
            solve_time_secs: None,
            opened_unix: None,
            solved_unix: None,
        }
    }

    pub const fn is_complete(&self) -> bool {
        matches!((self.puzzle_id, self.solve_time_secs), (Some(_), Some(_)))
    }

    pub fn update_stats(&mut self, stats: SolvedPuzzleStats) {
        self.solve_time_secs = Some(stats.solve_time);
        self.opened_unix = stats.opened;
        self.solved_unix = stats.solved;
    }
}

#[derive(Debug)]
pub struct Database {
    records: HashMap<NaiveDate, PuzzleStats>,
    filepath: PathBuf,
}

impl Database {
    /// Create a new database at the given path
    pub fn new<T: Into<PathBuf>>(out_path: T) -> Self {
        Self {
            records: HashMap::new(),
            filepath: out_path.into(),
        }
    }

    /// Load a database from file
    pub fn from_file<T: AsRef<Path>>(path: T) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .with_context(|| format!("Failed to open {}", path.to_str().unwrap()))?;
        let records = deserialize_records(file)?;
        Ok(Self {
            records,
            filepath: path.to_path_buf(),
        })
    }

    pub fn records(&self) -> Vec<PuzzleStats> {
        self.records.values().copied().collect()
    }

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
