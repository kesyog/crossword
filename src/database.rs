use crate::api_client::SolvedPuzzleStats;
use anyhow::{Context, Result};
use chrono::{naive::NaiveDate, Datelike, Duration, Weekday};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize, Serialize)]
pub struct PuzzleStats {
    pub date: NaiveDate,
    /// id used to identify a puzzle to NYT server
    /// TODO: consider removing Option wrapper once database has been fully updated with ids
    puzzle_id: Option<u32>,
    weekday: Weekday,
    // It would be nice to embed SolvedPuzzleStats here, but serde's flatten attribute doesn't play
    // well with the csv crate
    solve_time_secs: Option<u32>,
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
}

#[derive(Debug)]
pub struct Database {
    records: Vec<PuzzleStats>,
    // The csv crate's Writer will add a header row using struct fieldnames by default
    writer: Option<csv::Writer<File>>,
}

impl Database {
    pub fn new<T: AsRef<Path>>(output: T) -> Self {
        Self {
            records: Vec::new(),
            writer: Some(csv::Writer::from_path(output).unwrap()),
        }
    }

    pub fn from_file<T: AsRef<Path>, U: AsRef<Path>>(input: T, output: U) -> Result<Self> {
        let input = input.as_ref();
        let file = File::open(input)
            .with_context(|| format!("Failed to open {}", input.to_str().unwrap()))?;
        let records = deserialize_records(file)?;
        let mut writer = csv::Writer::from_path(output)?;
        for record in &records {
            writer.serialize(record)?;
        }
        writer.flush().expect("Flush error");
        Ok(Self {
            records,
            writer: Some(writer),
        })
    }

    pub fn search_space(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        max_step: u32,
    ) -> Vec<Vec<NaiveDate>> {
        let cache: HashSet<NaiveDate> = self.records.iter().map(|stats| stats.date).collect();
        let mut search_space: Vec<Vec<NaiveDate>> = Vec::new();
        let mut current_start = start;
        while current_start <= end {
            // Find next uncached date in range on or after current_start
            current_start = match current_start
                .iter_days()
                .take_while(|date| *date <= end)
                .find(|date| !cache.contains(date))
            {
                Some(date) => date,
                None => break,
            };
            let current_end =
                cmp::min(end, current_start + Duration::days(i64::from(max_step) - 1));
            // Filter any days that have already been cached out of the search block
            let search_block: Vec<NaiveDate> = current_start
                .iter_days()
                .take_while(|date| *date <= current_end)
                .filter(|date| !cache.contains(date))
                .collect();
            search_space.push(search_block);
            current_start = current_end + Duration::days(1);
        }

        search_space
    }

    pub fn add(&mut self, puzzle: PuzzleStats) {
        if let Some(writer) = self.writer.as_mut() {
            writer.serialize(puzzle).expect("Serialization error");
        }
        self.records.push(puzzle);
    }
}

fn deserialize_records<R: Read>(reader: R) -> Result<Vec<PuzzleStats>> {
    let reader = csv::Reader::from_reader(reader);
    let mut records: Vec<PuzzleStats> = Vec::new();
    for record in reader.into_deserialize() {
        records.push(record.with_context(|| "Malformed record")?);
    }

    Ok(records)
}
