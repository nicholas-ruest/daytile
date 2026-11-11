//! Deterministic calendar heatmaps from newline-delimited JSON values.

use chrono::{Datelike, Days, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::BufRead;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaytileError {
    #[error("line {line}: {source}")]
    Json {
        line: usize,
        source: serde_json::Error,
    },
    #[error("line {line}: value must be finite")]
    NonFinite { line: usize },
    #[error("input contains no events")]
    Empty,
    #[error("invalid date range for year {0}")]
    InvalidYear(i32),
    #[error("palette must contain exactly five #RRGGBB colors")]
    InvalidPalette,
    #[error("unknown aggregation '{0}' (expected sum, mean, or max)")]
    InvalidAggregation(String),
    #[error("unknown week start '{0}' (expected monday or sunday)")]
    InvalidWeekStart(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub date: NaiveDate,
    pub value: f64,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregation {
    Sum,
    Mean,
    Max,
}

impl FromStr for Aggregation {
    type Err = DaytileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "sum" => Ok(Self::Sum),
            "mean" => Ok(Self::Mean),
            "max" => Ok(Self::Max),
            _ => Err(DaytileError::InvalidAggregation(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekStart {
    Monday,
    Sunday,
}

impl WeekStart {
    fn row(self, weekday: Weekday) -> u32 {
        match self {
            Self::Monday => weekday.num_days_from_monday(),
            Self::Sunday => weekday.num_days_from_sunday(),
        }
    }

    fn offset_from_grid_start(self, weekday: Weekday) -> u64 {
        u64::from(self.row(weekday))
    }
}

impl FromStr for WeekStart {
    type Err = DaytileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "monday" => Ok(Self::Monday),
            "sunday" => Ok(Self::Sunday),
            _ => Err(DaytileError::InvalidWeekStart(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DayValue {
    pub date: NaiveDate,
    pub value: f64,
    pub event_count: usize,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub year: i32,
    pub aggregation: String,
    pub event_count: usize,
    pub populated_days: usize,
    pub total: f64,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub busiest_day: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
pub struct Calendar {
    pub year: i32,
    pub aggregation: Aggregation,
    pub event_count: usize,
    pub days: BTreeMap<NaiveDate, DayValue>,
}

#[derive(Debug, Clone)]
pub struct SvgOptions {
    pub title: String,
    pub week_start: WeekStart,
    pub palette: [String; 5],
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            title: "Calendar heatmap".to_owned(),
            week_start: WeekStart::Monday,
            palette: ["#dbeafe", "#93c5fd", "#3b82f6", "#1d4ed8", "#172554"].map(str::to_owned),
        }
    }
}

pub fn read_events(reader: impl BufRead) -> Result<Vec<Event>, DaytileError> {
    let mut events = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line
            .map_err(serde_json::Error::io)
            .map_err(|source| DaytileError::Json {
                line: line_number,
                source,
            })?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line).map_err(|source| DaytileError::Json {
            line: line_number,
            source,
        })?;
        if !event.value.is_finite() {
            return Err(DaytileError::NonFinite { line: line_number });
        }
        events.push(event);
    }
    if events.is_empty() {
        return Err(DaytileError::Empty);
    }
    Ok(events)
}

pub fn build_calendar(events: &[Event], year: i32, aggregation: Aggregation) -> Calendar {
    let mut grouped: BTreeMap<NaiveDate, Vec<&Event>> = BTreeMap::new();
    for event in events.iter().filter(|event| event.date.year() == year) {
        grouped.entry(event.date).or_default().push(event);
    }

    let days = grouped
        .into_iter()
        .map(|(date, entries)| {
            let value = match aggregation {
                Aggregation::Sum => entries.iter().map(|event| event.value).sum(),
                Aggregation::Mean => {
                    entries.iter().map(|event| event.value).sum::<f64>() / entries.len() as f64
                }
                Aggregation::Max => entries
                    .iter()
                    .map(|event| event.value)
                    .fold(f64::NEG_INFINITY, f64::max),
            };
            let labels = entries
                .iter()
                .filter_map(|event| event.label.clone())
                .collect();
            let day = DayValue {
                date,
                value,
                event_count: entries.len(),
                labels,
            };
            (date, day)
        })
        .collect();

    Calendar {
        year,
        aggregation,
        event_count: events
            .iter()
            .filter(|event| event.date.year() == year)
            .count(),
        days,
    }
}

impl Calendar {
    pub fn summary(&self) -> Summary {
        let minimum = self.days.values().map(|day| day.value).reduce(f64::min);
        let maximum = self.days.values().map(|day| day.value).reduce(f64::max);
        let busiest_day = self
            .days
            .values()
            .max_by(|left, right| {
                left.value
                    .total_cmp(&right.value)
                    .then_with(|| right.date.cmp(&left.date))
            })
            .map(|day| day.date);
        Summary {
            year: self.year,
            aggregation: match self.aggregation {
                Aggregation::Sum => "sum",
                Aggregation::Mean => "mean",
                Aggregation::Max => "max",
            }
            .to_owned(),
            event_count: self.event_count,
            populated_days: self.days.len(),
            total: self.days.values().map(|day| day.value).sum(),
            minimum,
            maximum,
            busiest_day,
        }
    }

    pub fn render_svg(&self, options: &SvgOptions) -> Result<String, DaytileError> {
        validate_palette(&options.palette)?;
        let first =
            NaiveDate::from_ymd_opt(self.year, 1, 1).ok_or(DaytileError::InvalidYear(self.year))?;
        let last = NaiveDate::from_ymd_opt(self.year, 12, 31)
            .ok_or(DaytileError::InvalidYear(self.year))?;
        let grid_start = first
            .checked_sub_days(Days::new(
                options.week_start.offset_from_grid_start(first.weekday()),
            ))
            .ok_or(DaytileError::InvalidYear(self.year))?;
        let week_count = ((last - grid_start).num_days() / 7 + 1) as u32;
        let cell = 14_u32;
        let gap = 3_u32;
        let step = cell + gap;
        let left = 64_u32;
        let top = 58_u32;
        let width = left + week_count * step + 26;
        let height = 244_u32;
        let values: Vec<f64> = self.days.values().map(|day| day.value).collect();
        let minimum = values.iter().copied().reduce(f64::min).unwrap_or(0.0);
        let maximum = values.iter().copied().reduce(f64::max).unwrap_or(0.0);
        let summary = self.summary();

        let mut svg = String::new();
        writeln!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc" viewBox="0 0 {width} {height}">"#
        )
        .expect("writing to String cannot fail");
        writeln!(
            svg,
            "  <title id=\"title\">{}</title>",
            escape(&options.title)
        )
        .expect("writing to String cannot fail");
        writeln!(
            svg,
            "  <desc id=\"desc\">{} populated days in {}; values range from {} to {}.</desc>",
            summary.populated_days,
            self.year,
            format_number(minimum),
            format_number(maximum)
        )
        .expect("writing to String cannot fail");
        svg.push_str("  <style>text{font-family:ui-sans-serif,system-ui,sans-serif;fill:#334155}.day{stroke:#fff;stroke-width:1}.empty{fill:#f1f5f9}</style>\n");
        writeln!(
            svg,
            "  <rect width=\"100%\" height=\"100%\" rx=\"12\" fill=\"#ffffff\"/>"
        )
        .expect("writing to String cannot fail");
        writeln!(
            svg,
            "  <text x=\"20\" y=\"28\" font-size=\"17\" font-weight=\"700\">{}</text>",
            escape(&options.title)
        )
        .expect("writing to String cannot fail");

        let day_labels = match options.week_start {
            WeekStart::Monday => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            WeekStart::Sunday => ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
        };
        for (row, label) in day_labels.iter().enumerate() {
            let y = top + row as u32 * step + 11;
            writeln!(
                svg,
                "  <text x=\"18\" y=\"{y}\" font-size=\"10\">{label}</text>"
            )
            .expect("writing to String cannot fail");
        }

        let mut date = first;
        while date <= last {
            let column = ((date - grid_start).num_days() / 7) as u32;
            let row = options.week_start.row(date.weekday());
            let x = left + column * step;
            let y = top + row * step;
            if let Some(day) = self.days.get(&date) {
                let level = color_level(day.value, minimum, maximum);
                let fill = &options.palette[level];
                let labels = if day.labels.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", day.labels.join(", "))
                };
                writeln!(svg, "  <rect class=\"day\" data-date=\"{date}\" data-value=\"{}\" x=\"{x}\" y=\"{y}\" width=\"{cell}\" height=\"{cell}\" rx=\"2\" fill=\"{fill}\"><title>{date}: {} ({} event{}){}</title></rect>", format_number(day.value), format_number(day.value), day.event_count, if day.event_count == 1 { "" } else { "s" }, escape(&labels)).expect("writing to String cannot fail");
            } else {
                writeln!(svg, "  <rect class=\"day empty\" data-date=\"{date}\" x=\"{x}\" y=\"{y}\" width=\"{cell}\" height=\"{cell}\" rx=\"2\"><title>{date}: no data</title></rect>").expect("writing to String cannot fail");
            }
            date = date
                .succ_opt()
                .ok_or(DaytileError::InvalidYear(self.year))?;
        }

        let legend_y = top + 7 * step + 26;
        writeln!(
            svg,
            "  <text x=\"{left}\" y=\"{legend_y}\" font-size=\"10\">Less</text>"
        )
        .expect("writing to String cannot fail");
        for (index, color) in options.palette.iter().enumerate() {
            let x = left + 34 + index as u32 * step;
            writeln!(svg, "  <rect x=\"{x}\" y=\"{}\" width=\"{cell}\" height=\"{cell}\" rx=\"2\" fill=\"{color}\"/>", legend_y - 11).expect("writing to String cannot fail");
        }
        writeln!(
            svg,
            "  <text x=\"{}\" y=\"{legend_y}\" font-size=\"10\">More</text>",
            left + 34 + 5 * step
        )
        .expect("writing to String cannot fail");
        svg.push_str("</svg>\n");
        Ok(svg)
    }
}

fn validate_palette(palette: &[String; 5]) -> Result<(), DaytileError> {
    if palette.iter().all(|color| {
        color.len() == 7
            && color.starts_with('#')
            && color[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    }) {
        Ok(())
    } else {
        Err(DaytileError::InvalidPalette)
    }
}

fn color_level(value: f64, minimum: f64, maximum: f64) -> usize {
    if maximum <= minimum {
        return 4;
    }
    let normalized = ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0);
    (normalized * 4.0).round() as usize
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.6}");
        formatted.trim_end_matches('0').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn events() -> Vec<Event> {
        read_events(Cursor::new(
            r#"{"date":"2024-01-01","value":2,"label":"alpha"}
{"date":"2024-01-01","value":4,"label":"beta"}
{"date":"2024-02-29","value":9}
{"date":"2025-01-01","value":100}
"#,
        ))
        .unwrap()
    }

    #[test]
    fn aggregates_duplicate_dates_without_crossing_years() {
        let calendar = build_calendar(&events(), 2024, Aggregation::Mean);
        assert_eq!(calendar.event_count, 3);
        assert_eq!(calendar.days.len(), 2);
        assert_eq!(
            calendar.days[&NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()].value,
            3.0
        );
        assert_eq!(
            calendar.summary().busiest_day,
            NaiveDate::from_ymd_opt(2024, 2, 29)
        );
    }

    #[test]
    fn renders_every_day_of_a_leap_year() {
        let calendar = build_calendar(&events(), 2024, Aggregation::Sum);
        let svg = calendar.render_svg(&SvgOptions::default()).unwrap();
        assert_eq!(svg.matches("class=\"day").count(), 366);
        assert!(svg.contains("data-date=\"2024-02-29\""));
        assert!(svg.contains("aria-labelledby=\"title desc\""));
    }

    #[test]
    fn escapes_titles_and_labels() {
        let calendar = build_calendar(&events(), 2024, Aggregation::Sum);
        let options = SvgOptions {
            title: "A & B <report>".to_owned(),
            ..SvgOptions::default()
        };
        let svg = calendar.render_svg(&options).unwrap();
        assert!(svg.contains("A &amp; B &lt;report&gt;"));
    }

    #[test]
    fn rejects_empty_input() {
        let error = read_events(Cursor::new("\n")).unwrap_err();
        assert!(matches!(error, DaytileError::Empty));
    }

    #[test]
    fn validates_palette_shape() {
        let calendar = build_calendar(&events(), 2024, Aggregation::Sum);
        let mut options = SvgOptions::default();
        options.palette[0] = "blue".to_owned();
        assert!(matches!(
            calendar.render_svg(&options),
            Err(DaytileError::InvalidPalette)
        ));
    }
}
