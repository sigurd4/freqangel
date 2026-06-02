use core::{num::{ParseFloatError, ParseIntError}, ops::{Deref, DerefMut, Range}, str::{FromStr, ParseBoolError}};
use std::{fs::File, io::{BufRead, BufReader}, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

use isahc::{HttpClient, ReadResponseExt, auth::{Authentication, Credentials}, config::Configurable};
use regex::Regex;

use crate::ApplicationError::ParseFreqs;

#[derive(Default, PartialEq, PartialOrd, Clone)]
struct Freq
{
    freq_hertz: Option<u64>,
    notes: Notes,
    enabled: Option<bool>,
    channel: Option<String>,
    channel_bandwidth: ChannelBandwidth,
    threshold: Threshold,
    squelch: Squelch
}

#[derive(Default, PartialEq, PartialOrd, Clone)]
struct Squelch
{
    decibel: Option<f64>
}

impl Squelch
{
    const DEFAULT: f64 = -120.0;
}

impl core::fmt::Display for Squelch
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let Self { decibel } = self;

        if let Some(decibel) = decibel
        {
            write!(f, "{decibel}")
        }
        else
        {
            write!(f, "{}", Self::DEFAULT)
        }
    }
}

#[derive(Default, PartialEq, PartialOrd, Clone)]
struct Threshold
{
    decibel: Option<f64>
}

impl Threshold
{
    const DEFAULT: f64 = -70.0;
}

impl core::fmt::Display for Threshold
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let Self { decibel } = self;

        if let Some(decibel) = decibel
        {
            write!(f, "{decibel}")
        }
        else
        {
            write!(f, "{}", Self::DEFAULT)
        }
    }
}

#[derive(Default, PartialEq, PartialOrd, Clone)]
struct ChannelBandwidth
{
    hertz: Option<u32>
}

impl ChannelBandwidth
{
    const DEFAULT: u32 = 8000;
}

impl core::fmt::Display for ChannelBandwidth
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let Self { hertz } = self;

        if let Some(hertz) = hertz
        {
            write!(f, "{hertz}")
        }
        else
        {
            write!(f, "{}", Self::DEFAULT)
        }
    }
}

#[derive(PartialEq, PartialOrd, Clone, Copy)]
struct Days
{
    from: u8,
    to: Option<u8>
}

impl core::fmt::Display for Days
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let Self { from, to } = self;

        write!(f, "{from}")?;

        if let Some(to) = to
        {
            write!(f, "-{to}")?
        }

        Ok(())
    }
}

impl Days
{
    fn today() -> u8
    {
        let now = SystemTime::now();
        let secs = now.duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        ((secs / 86400 + 4) % 7 + 1) as u8
    }

    fn is_now(&self) -> bool
    {
        let today = Self::today();

        let Self { from, to } = self;

        match to
        {
            None => (*from + 6) % 7 + 1 == today,
            Some(to) => ((*from + 6) % 7 + 1..=(*to + 6) % 7 + 1).contains(&today)
        }
    }

    fn from_str(s: &str) -> Result<Vec<Self>, ParseFreqsError>
    {
        let s = s.trim();

        let mut segments = s.chars()
            .map(|c| c.to_string())
            .peekable();

        let mut days = Vec::new();
        while let Some(segment) = segments.next()
        {
            let from = u8::from_str(&segment)
                .map_err(|error| ParseFreqsError::ParseDay(error, s.to_string()))?;

            days.push(Days {
                from,
                to: if segments.peek().as_ref().map(|s| s.as_str()) == Some("-")
                {
                    let _ = segments.next()
                        .ok_or_else(|| ParseFreqsError::ExpectedDash(s.to_string()))?;
                    let next_segment = segments.next()
                        .unwrap_or("7".to_string());
                    let to = u8::from_str(&next_segment)
                        .map_err(|error| ParseFreqsError::ParseDay(error, s.to_string()))?;
                    Some(to)
                }
                else
                {
                    None
                }
            })
        }
        Ok(days)
    }
}

#[derive(Default, Clone)]
struct Notes
{
    uti: Option<String>,
    station: Option<String>,
    language: Option<String>,
    location: Option<String>
}

impl PartialEq for Notes
{
    fn eq(&self, other: &Self) -> bool
    {
        self.to_string() == other.to_string()
    }
}
impl PartialOrd for Notes
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering>
    {
        self.to_string().partial_cmp(&other.to_string())
    }
}

impl core::fmt::Display for Notes
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let Self { uti, station, language, location } = self;

        if let Some(uti) = uti
        {
            write!(f, "[{uti}] ")?
        }
        if let Some(station) = station
        {
            write!(f, "{station}")?;
        }
        else
        {
            write!(f, "???")?
        }
        if let Some(language) = language
        {
            write!(f, " ({language})")?
        }
        if let Some(location) = location
        {
            write!(f, " [{location}]")?
        }

        Ok(())
    }
}

impl Eq for Freq
{

}

impl Ord for Freq
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering
    {
        self.partial_cmp(other)
            .unwrap_or(core::cmp::Ordering::Equal)
    }
}

impl Freq
{
    fn set_value<T>(dst: &mut Option<T>, value: T) -> Result<(), ParseFreqsError>
    {
        match dst
        {
            Some(_) => Err(ParseFreqsError::AlreadyDefined),
            None => Ok(*dst = Some(value))
        }
    }
}

#[derive(PartialEq, PartialOrd, Clone)]
struct TimeUtc
{
    from: u16,
    to: u16
}

impl TimeUtc
{
    fn now() -> u16
    {
        let now = SystemTime::now();
        let secs = now.duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hours = ((secs / 60 / 60) % 24) as u16;
        let minutes = ((secs / 60) % 60) as u16;

        (hours*100 + minutes) % 2400
    }

    fn is_now(&self) -> bool
    {
        let now = Self::now();

        (self.from..self.to).contains(&now)
    }
}

impl FromStr for TimeUtc
{
    type Err = ParseFreqsError;

    fn from_str(s: &str) -> Result<Self, Self::Err>
    {
        if s == "0."
        {
            return Ok(Self {
                from: 0000,
                to: 0000
            })
        }

        let [from, to] = s.split("-")
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| ParseFreqsError::TooManyDashes(s.to_string()))?;

        Ok(Self {
            from: u16::from_str(from)
                .map_err(|error| ParseFreqsError::ParseTimeUtc(error, s.to_string()))?,
            to: u16::from_str(to)
                .map_err(|error| ParseFreqsError::ParseTimeUtc(error, s.to_string()))?
        })
    }
}

impl core::fmt::Display for TimeUtc
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}-{}", self.from % 2400, self.to % 2400)
    }
}

struct Freqs
{
    file_type: FileType,
    freqs: Vec<Freq>
}

#[derive(Debug)]
enum ParseFreqsError
{
    ExpectedCsvHeader,
    FailedRead(std::io::Error),
    ParseHertz(ParseIntError, String),
    ParseBandwidthHertz(ParseIntError, String),
    ParseEnabled(ParseBoolError, String),
    ParseDecibel(ParseFloatError, String),
    ParseDay(ParseIntError, String),
    ParseTimeUtc(ParseIntError, String),
    ExpectedDash(String),
    UnexpectedCsvHeader(String),
    TooManyDashes(String),
    AlreadyDefined
}

#[derive(Debug)]
struct FormatFreqsError(pub std::io::Error);

struct CsvTitle
{
    title: &'static str,
    getter: &'static dyn Fn(&Freq) -> String,
    setter: &'static dyn Fn(&mut Freq, &str) -> Result<(), ParseFreqsError>
}

struct CsvTitleFilter
{
    title: &'static str,
    filter: &'static dyn Fn(&str) -> Result<bool, ParseFreqsError>
}

impl Freqs
{
    const SDRANGEL_CSV_HEADER: &[CsvTitle] = &[
        CsvTitle {
            title: "Freq (Hz)",
            getter: &|freq| freq.freq_hertz.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.freq_hertz, u64::from_str(field).map_err(|error| ParseFreqsError::ParseHertz(error, field.to_string()))?)
        },
        CsvTitle {
            title: "Enable",
            getter: &|freq| freq.enabled.unwrap_or(true).to_string(),
            setter: &|freq, field| Freq::set_value(&mut freq.enabled, bool::from_str(field).map_err(|error| ParseFreqsError::ParseEnabled(error, field.to_string()))?)
        },
        CsvTitle {
            title: "Notes",
            getter: &|freq| freq.notes.to_string(),
            setter: &|freq, field| Freq::set_value(&mut freq.notes.station, field.to_string())
        },
        CsvTitle {
            title: "Channel",
            getter: &|freq| freq.channel.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.channel, field.to_string())
        },
        CsvTitle {
            title: "Ch BW (Hz)",
            getter: &|freq| freq.channel_bandwidth.to_string(),
            setter: &|freq, field| Freq::set_value(&mut freq.channel_bandwidth.hertz, u32::from_str(field).map_err(|error| ParseFreqsError::ParseBandwidthHertz(error, field.to_string()))?)
        },
        CsvTitle {
            title: "TH (dB)",
            getter: &|freq| freq.threshold.to_string(),
            setter: &|freq, field| Freq::set_value(&mut freq.threshold.decibel, f64::from_str(field).map_err(|error| ParseFreqsError::ParseDecibel(error, field.to_string()))?)
        },
        CsvTitle {
            title: "Sq (dB)",
            getter: &|freq| freq.squelch.to_string(),
            setter: &|freq, field| Freq::set_value(&mut freq.squelch.decibel, f64::from_str(field).map_err(|error| ParseFreqsError::ParseDecibel(error, field.to_string()))?)
        }
    ];
    const PERSEUS_TXT_HEADER: &[CsvTitle] = &[
        CsvTitle {
            title: "kHz",
            getter: &|freq| freq.freq_hertz.as_ref().map(|&v| (v/1000).to_string()).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.freq_hertz, u64::from_str(field).map_err(|error| ParseFreqsError::ParseHertz(error, field.to_string()))?*1000)
        },
        CsvTitle {
            title: "ITU",
            getter: &|freq| freq.notes.uti.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.notes.uti, field.to_string())
        },
        CsvTitle {
            title: "Station",
            getter: &|freq| freq.notes.station.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.notes.station, field.to_string())
        },
        CsvTitle {
            title: "Lang.",
            getter: &|freq| freq.notes.language.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.notes.language, field.to_string())
        },
        CsvTitle {
            title: "Location",
            getter: &|freq| freq.notes.location.as_ref().map(ToString::to_string).unwrap_or_default(),
            setter: &|freq, field| Freq::set_value(&mut freq.notes.location, field.to_string())
        }
    ];

    const PERSEUS_TXT_HEADER_FILTERS: &[CsvTitleFilter] = &[
        CsvTitleFilter {
            title: "Time(UTC)",
            filter: &|field| {
                let time = TimeUtc::from_str(field)?;

                Ok(time.is_now())
            }
        },
        CsvTitleFilter {
            title: "",
            filter: &|_field| Ok(true)
        },
        CsvTitleFilter {
            title: "Days",
            filter: &|field| {
                let days = Days::from_str(field)?;

                Ok(days.iter().any(|days| days.is_now()))
            }
        }
    ];

    pub fn read<R>(reader: R, file_type: FileType) -> Result<Self, ParseFreqsError>
    where
        R: std::io::Read
    {
        let freqs = match file_type
        {
            FileType::SdrAngelCsv => {
                let buf_reader = BufReader::new(reader);
                let mut lines = buf_reader.lines()
                    .map(|result| result.map_err(|error| ParseFreqsError::FailedRead(error)));

                let header = lines.next()
                    .ok_or(ParseFreqsError::ExpectedCsvHeader)??
                    .split(",")
                    .map(str::trim)
                    .map(|field| {
                        for title in Self::SDRANGEL_CSV_HEADER.iter()
                        {
                            if field == title.title
                            {
                                return Ok(title.setter)
                            }
                        }

                        Err(ParseFreqsError::UnexpectedCsvHeader(field.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                lines.filter(|line| !line.as_ref().is_ok_and(|line| line.trim().is_empty()))
                    .map(|line| {
                        let mut freq = Freq::default();
                        for (field, parser) in line?.split(",")
                            .map(str::trim)
                            .zip(header.iter())
                            .filter(|(field, _)| !field.trim().is_empty())
                        {
                            parser(&mut freq, field)?
                        }
                        Ok(freq)
                    }).collect::<Result<Vec<_>, _>>()?
            },
            FileType::PerseusTxt => {
                let buf_reader = BufReader::new(reader);
                let mut lines = buf_reader.lines()
                    .map(|result| result.map_err(|error| ParseFreqsError::FailedRead(error)))
                    .peekable();

                let mut header = lines.next()
                    .ok_or(ParseFreqsError::ExpectedCsvHeader)??;
                let mut header_indices = Vec::new();
                while let Some(Ok(peek)) = lines.peek()
                {
                    let mut is_bar = true;
                    for (i, c) in peek.trim()
                        .chars()
                        .enumerate()
                    {
                        if c == '+'
                        {
                            header_indices.push(i)
                        }
                        else if c != '-'
                        {
                            is_bar = false;
                            break
                        }
                    }
                    let len = peek.len();
                    let line = lines.next()
                        .ok_or(ParseFreqsError::ExpectedCsvHeader)??;
                    if is_bar
                    {
                        header_indices.push(len);
                        break
                    }
                    else
                    {
                        header = line
                    }
                }

                let header_indices = header_indices.array_windows()
                    .map(|[i0, i1]| *i0..*i1)
                    .collect::<Vec<_>>();

                enum HeaderAction
                {
                    Field(&'static dyn Fn(&mut Freq, &str) -> Result<(), ParseFreqsError>),
                    Filter(&'static dyn Fn(&str) -> Result<bool, ParseFreqsError>)
                }

                let header = header_indices.iter()
                    .map(|i| i.start.min(header.len())..i.end.min(header.len()))
                    .map(|mut i| {
                        while &header[i.start..=i.start] != " "
                            && let Some(is1) = i.start.checked_sub(1)
                            && &header[is1..=is1] != " "
                        {
                            i.start = is1
                        }

                        header[i].trim()
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                    })
                    .map(|field| {
                        for title in Self::PERSEUS_TXT_HEADER_FILTERS.iter()
                        {
                            if field == title.title
                            {
                                return Ok(HeaderAction::Filter(title.filter))
                            }
                        }
                        for title in Self::PERSEUS_TXT_HEADER.iter()
                        {
                            if field == title.title
                            {
                                return Ok(HeaderAction::Field(title.setter))
                            }
                        }

                        Err(ParseFreqsError::UnexpectedCsvHeader(field.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                lines.filter(|line| !line.as_ref().is_ok_and(|line| line.trim().is_empty()))
                    .filter_map(|line| {
                        let mut freq = Freq::default();
                        let line = match line
                        {
                            Ok(line) => line,
                            Err(error) => return Some(Err(error))
                        };
                        let len = line.len();
                        for (field, action) in header_indices.iter()
                            .map(|i| i.start.min(len)..i.end.min(len))
                            .zip(header.iter())
                            .filter(|(i, _)| i.start < i.end)
                            .map(|(i, header)| (line[i].trim(), header))
                            .filter(|(field, _)| !field.trim().is_empty())
                        {
                            match action
                            {
                                HeaderAction::Field(parser) => match parser(&mut freq, field)
                                {
                                    Ok(()) => (),
                                    Err(error) => return Some(Err(error))
                                },
                                HeaderAction::Filter(filter) => if match filter(field)
                                {
                                    Ok(pass) => !pass,
                                    Err(error) => return Some(Err(error))
                                }
                                {
                                    return None
                                }
                            }
                            
                        }
                        Some(Ok(freq))
                    }).collect::<Result<Vec<_>, _>>()?
            }
        };

        Ok(Self {
            file_type,
            freqs
        })
    }

    pub fn write<W>(&self, mut writer: W) -> Result<(), FormatFreqsError>
    where
        W: std::io::Write
    {
        let Self { file_type, freqs } = self;

        match file_type
        {
            FileType::SdrAngelCsv => {
                let header = Self::SDRANGEL_CSV_HEADER.iter()
                    .map(|title| title.title.replace(",", "_").replace("\"", "_"))
                    .collect::<Vec<_>>()
                    .join(",");
                writeln!(writer, "{header}").map_err(FormatFreqsError)?;

                for freq in freqs
                {
                    let line = Self::SDRANGEL_CSV_HEADER.iter()
                        .map(|title| (title.getter)(freq).replace(",", "_").replace("\"", "_"))
                        .collect::<Vec<_>>()
                        .join(",");
                    writeln!(writer, "{line}").map_err(FormatFreqsError)?;
                }
            },
            FileType::PerseusTxt => todo!()
        }

        Ok(())
    }

    fn expect(args: &mut impl Iterator<Item: Into<String>>) -> Result<(PathBuf, Freqs), ApplicationError>
    {
        let arg = args.next()
            .map(|arg| arg.into())
            .ok_or(ApplicationError::ExpectedFilePath)?;

        let path = PathBuf::from_str(&arg)
            .unwrap_or_else(|error| match error {});
        
        let freqs = Freqs::read_path(&path)?;

        Ok((path, freqs))
    }

    fn read_path(path: &Path) -> Result<Freqs, ApplicationError>
    {
        let file_type = match path.extension()
        {
            None => FileType::SdrAngelCsv,
            Some(extension) => {
                let extension = extension.to_string_lossy();
                match extension.as_ref()
                {
                    "csv" => FileType::SdrAngelCsv,
                    _ => return Err(ApplicationError::FileTypeUnrecognized(extension.into_owned()))
                }
            }
        };

        if !path.is_file()
        {
            return Err(ApplicationError::FileNonexistant {
                path: path.into(),
                file_type
            })
        }

        let file = File::open(&path)
            .map_err(ApplicationError::FailOpen)?;

        let freqs = Freqs::read(file, file_type)?;

        Ok(freqs)
    }

    fn write_path(&self, path: &Path) -> Result<(), ApplicationError>
    {
        let file_type = match path.extension()
        {
            None => FileType::SdrAngelCsv,
            Some(extension) => {
                let extension = extension.to_string_lossy();
                match extension.as_ref()
                {
                    "csv" => FileType::SdrAngelCsv,
                    _ => return Err(ApplicationError::FileTypeUnrecognized(extension.into_owned()))
                }
            }
        };

        let file = File::create(&path)
            .map_err(ApplicationError::FailCreate)?;

        Self {
            file_type,
            freqs: self.freqs.clone()
        }.write(file)?;

        Ok(())
    }

    fn dedup(&mut self)
    {
        fn merge_float(b: Option<f64>, a: Option<f64>) -> Option<f64>
        {
            match (b, a)
            {
                (Some(dud), o) | (o, Some(dud)) if !dud.is_finite() => o,
                (Some(b), Some(a)) => Some(b.midpoint(a)),
                (None, o) | (o, None) => o
            }
        }

        fn merge_u32(b: Option<u32>, a: Option<u32>) -> Option<u32>
        {
            match (b, a)
            {
                (Some(b), Some(a)) => Some(b.midpoint(a)),
                (None, o) | (o, None) => o
            }
        }

        self.sort();
        self.freqs.dedup_by(|a, b| {
            if a.freq_hertz.is_none()
            {
                return true
            }
            if a.freq_hertz == b.freq_hertz && a.notes.to_string()
                .replace(",", "_")
                .replace("\"", "_")
                .trim()
                == b.notes.to_string()
                    .replace(",", "_")
                    .replace("\"", "_")
                    .trim()
            {
                *b = Freq {
                    freq_hertz: b.freq_hertz,
                    notes: if a.notes > b.notes { a.notes.clone() } else { b.notes.clone() },
                    enabled: match (b.enabled, a.enabled)
                    {
                        (Some(b), Some(a)) => Some(b || a),
                        (None, o) | (o, None) => o
                    },
                    channel: match (b.channel.clone(), a.channel.clone())
                    {
                        (Some(chosen), None) => Some(chosen),
                        (_, o) => o
                    },
                    channel_bandwidth: ChannelBandwidth {
                        hertz: merge_u32(b.channel_bandwidth.hertz, a.channel_bandwidth.hertz)
                    },
                    threshold: Threshold {
                        decibel: merge_float(b.threshold.decibel, a.threshold.decibel)
                    },
                    squelch: Squelch {
                        decibel: merge_float(b.squelch.decibel, a.squelch.decibel)
                    }
                };
                true
            }
            else
            {
                false
            }
        });
    }

    fn sort(&mut self)
    {
        self.freqs.sort();
    }

    fn merge(&mut self, other: Self)
    {
        self.freqs.extend(other.freqs);
        self.dedup();
    }
}

enum CsvType
{
    SdrAngel
}

#[derive(Debug)]
enum FileType
{
    SdrAngelCsv,
    PerseusTxt
}

#[derive(Debug)]
enum ApplicationError
{
    ExpectedApplication,
    ExpectedWord(String),
    WordsDiffer(String, String),
    ExpectedAction,
    ExpectedFilePath,
    FileNonexistant {
        path: PathBuf,
        file_type: FileType
    },
    FailOpen(std::io::Error),
    FailCreate(std::io::Error),
    FailDelete(std::io::Error),
    FileTypeUnrecognized(String),
    ParseAction(ParseActionError),
    ParseFreqs(ParseFreqsError),
    FormatFreqs(FormatFreqsError),
    FetchWebData(isahc::Error),
    ReadWebData(std::io::Error),
    FindWebData,
    FailedRegex(regex::Error)
}

#[derive(Debug)]
struct ParseActionError
{
    arg: String
}

impl From<ParseActionError> for ApplicationError
{
    fn from(error: ParseActionError) -> Self
    {
        ApplicationError::ParseAction(error)
    }
}

impl From<ParseFreqsError> for ApplicationError
{
    fn from(error: ParseFreqsError) -> Self
    {
        ApplicationError::ParseFreqs(error)
    }
}

impl From<FormatFreqsError> for ApplicationError
{
    fn from(error: FormatFreqsError) -> Self
    {
        ApplicationError::FormatFreqs(error)
    }
}

impl From<isahc::Error> for ApplicationError
{
    fn from(error: isahc::Error) -> Self
    {
        ApplicationError::FetchWebData(error)
    }
}
impl From<regex::Error> for ApplicationError
{
    fn from(error: regex::Error) -> Self
    {
        ApplicationError::FailedRegex(error)
    }
}

enum Action
{
    Sort,
    Dedup,
    Fetch,
    Merge
}

impl Action
{
    fn expect(args: &mut impl Iterator<Item: Into<String>>) -> Result<Action, ApplicationError>
    {
        let arg = args.next()
            .map(|arg| arg.into())
            .ok_or(ApplicationError::ExpectedAction)?;

        Ok(
            Action::from_str(&arg)?
        )
    }
}

impl FromStr for Action
{
    type Err = ParseActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err>
    {
        match s
        {
            "sort" => Ok(Action::Sort),
            "dedup" => Ok(Action::Dedup),
            "merge" => Ok(Action::Merge),
            "fetch" => Ok(Action::Fetch),
            _ => Err(ParseActionError {
                arg: s.to_string()
            })
        }
    }
}

fn main() -> Result<(), ApplicationError>
{
    run(std::env::args())
}

fn expect_application(args: &mut impl Iterator<Item: Into<String>>) -> Result<(), ApplicationError>
{
    let _ = args.next()
        .ok_or(ApplicationError::ExpectedApplication)?;

    Ok(())
}

fn expect_word(args: &mut impl Iterator<Item: Into<String>>, word: &str) -> Result<(), ApplicationError>
{
    let arg = args.next()
        .ok_or_else(|| ApplicationError::ExpectedWord(word.to_string()))?
        .into();

    if arg != word
    {
        return Err(ApplicationError::WordsDiffer(arg, word.to_string()))
    }

    Ok(())
}

fn run(args: impl IntoIterator<Item: Into<String> + Clone>) -> Result<(), ApplicationError>
{
    let mut args = args.into_iter();

    expect_application(&mut args)?;

    let action = Action::expect(&mut args)?;

    match action
    {
        Action::Sort => {
            let (path, mut freqs) = Freqs::expect(&mut args)?;
            freqs.sort();
            freqs.write_path(&path)?;
            
            let mut args = args.peekable();
            while args.peek().is_some()
            {
                let (path, mut freqs) = Freqs::expect(&mut args)?;
                freqs.sort();
                freqs.write_path(&path)?;
            }
        },
        Action::Dedup => {
            let (path, mut freqs) = Freqs::expect(&mut args)?;
            freqs.dedup();
            freqs.write_path(&path)?;
            
            let mut args = args.peekable();
            while args.peek().is_some()
            {
                let (path, mut freqs) = Freqs::expect(&mut args)?;
                freqs.dedup();
                freqs.write_path(&path)?;
            }
        },
        Action::Merge => {
            let mut args = args.peekable();

            let mut take = if let Some(next_arg) = args.peek() && next_arg.clone().into() == "take"
            {
                expect_word(&mut args, "take")?;
                Some(Vec::new())
            }
            else
            {
                None
            };

            let (path, freqs_from) = Freqs::expect(&mut args)?;
            let mut freqs = vec![freqs_from];
            if let Some(take) = take.as_mut()
            {
                take.push(path)
            }

            while let Some(next_arg) = args.peek() && next_arg.clone().into() != "into"
            {
                if next_arg.clone().into() == "take"
                {
                    expect_word(&mut args, "take")?;
                    if take.is_some()
                    {
                        return Err(ApplicationError::ExpectedFilePath)
                    }
                    take = Some(Vec::new())
                }
                let (path, freqs_from) = Freqs::expect(&mut args)?;
                freqs.push(freqs_from);
                if let Some(take) = take.as_mut()
                {
                    take.push(path)
                }
            }

            expect_word(&mut args, "into")?;
            let (path, mut freqs_into) = Freqs::expect(&mut args)?;
            
            for freqs_from in freqs
            {
                freqs_into.merge(freqs_from)
            }
            freqs_into.write_path(&path)?;

            if let Some(take) = take
            {
                for path_taken in take
                {
                    if path != path_taken
                    {
                        std::fs::remove_file(path_taken)
                            .map_err(ApplicationError::FailDelete)?
                    }
                }
            }
        },
        Action::Fetch => {
            let mut args = args.peekable();
            let into = if let Some(next_arg) = args.peek() && next_arg.clone().into() == "into"
            {
                expect_word(&mut args, "into")?;
                true
            }
            else
            {
                false
            };

            let (path, mut freqs_into) = Freqs::expect(&mut args)
                .or_else(|error| match error
                {
                    ApplicationError::FileNonexistant {
                        path,
                        file_type
                    } => Ok((path, Freqs {
                        file_type,
                        freqs: Vec::new()
                    })),
                    error => Err(error)
                })?;
            if !into && path.exists()
            {
                return Err(ApplicationError::ExpectedWord("into".into()))
            }

            let front_page = isahc::get("https://www1.s2.starcat.ne.jp/ndxc/nnk.htm")?
                .text()
                .map_err(ApplicationError::ReadWebData)?;

            let regex = Regex::new(r"https:\/\/www1\.m2\.mediacat\.ne\.jp\/binews\/[a-z][a-z]\/[a-z][a-z]\/userlist1\.txt")?;

            let userlist_url = regex.find(&front_page)
                .ok_or(ApplicationError::FindWebData)?
                .as_str();

            let userlist = isahc::get(userlist_url)?
                .text()
                .map_err(ApplicationError::ReadWebData)?;

            let freqs = Freqs::read(userlist.as_bytes(), FileType::PerseusTxt)?;
            freqs_into.merge(freqs);
            freqs_into.write_path(&path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test
{
    use core::str::FromStr;

    #[test]
    fn test_fetch()
    {
        crate::run(["freqangel", "fetch", "into", "target/userlist1.csv"]).unwrap()
    }
}