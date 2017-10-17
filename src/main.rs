extern crate clap;
use clap::{Arg, App};

extern crate termion;
use termion::color;
use termion::terminal_size;

extern crate serde_yaml;
extern crate serde;
use serde::de::{self, Visitor, Deserialize, Deserializer};

extern crate unicode_segmentation;
use unicode_segmentation::UnicodeSegmentation;

use std::path;
use std::env;
use std::fs;
use std::fmt;
use std::ffi;

use std::cmp::max;
use std::cmp::Ordering;
use std::collections::HashMap;

extern crate num_iter;
use num_iter::range_step;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Verbosity {
    Quiet,
    Warn,
    Debug,
}

type Options = HashMap<String, String>;

#[derive(Debug)]
struct Config {
    files: Options,
    file_aliases: Options,
    folders: Options,
    folder_aliases: Options,
    colors: HashMap<ColorType, RealColor>,
    max_width: usize,
    printer: Box<EntryPrinter>,
}

#[derive(Hash, Debug, PartialEq, Eq, Clone, Copy)]
enum ColorType {
    UnrecognizedFile,
    RecognizedFile,
    Dir,
    DeadLink,
    Link,
    Write,
    Read,
    Exec,
    NoAccess,
    DayOld,
    HourOld,
    NoModifier,
    Report,
    User,
    Tree,
    Empty,
    Normal,
}

struct ColorTypeVisitor;
impl Visitor for ColorTypeVisitor {
    type Value = ColorType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("one of unrecognized_file, recognized_file, dir, dead_link, link, write, read, exec, no_access, day_old, hour_old, no_modifier, report, user, tree, empty, normal")
    }

    fn visit_str<E>(self, value: &str) -> Result<ColorType, E>
        where E: de::Error
    {
        match value {
            "unrecognized_file" => Ok(ColorType::UnrecognizedFile),
            "recognized_file" => Ok(ColorType::RecognizedFile),
            "dir" => Ok(ColorType::Dir),
            "dead_link" => Ok(ColorType::DeadLink),
            "link" => Ok(ColorType::Link),
            "write" => Ok(ColorType::Write),
            "read" => Ok(ColorType::Read),
            "exec" => Ok(ColorType::Exec),
            "no_access" => Ok(ColorType::NoAccess),
            "day_old" => Ok(ColorType::DayOld),
            "hour_old" => Ok(ColorType::HourOld),
            "no_modifier" => Ok(ColorType::NoModifier),
            "report" => Ok(ColorType::Report),
            "user" => Ok(ColorType::User),
            "tree" => Ok(ColorType::Tree),
            "empty" => Ok(ColorType::Empty),
            "normal" => Ok(ColorType::Normal),
            _ => Err(E::custom(format!("Unknown ColorType: {}", value)))
        }
    }
}

impl Deserialize for ColorType {
    fn deserialize<D>(deserializer: D) -> Result<ColorType, D::Error>
        where D: Deserializer
    {
        deserializer.deserialize_str(ColorTypeVisitor)
    }
}

#[derive(Hash, Debug, PartialEq, Eq, Clone, Copy)]
enum RealColor {
    Yellow,
    Green,
    Blue,
    Red,
    Cyan,
    Magenta,
    Grey,
    White,
    Black,
}

struct RealColorVisitor;
impl Visitor for RealColorVisitor {
    type Value = RealColor;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("one of yellow, green, blue, red, cyan, magenta, grey, white, black")
    }

    fn visit_str<E>(self, value: &str) -> Result<RealColor, E>
        where E: de::Error
    {
        match value {
            "yellow" => Ok(RealColor::Yellow),
            "green" => Ok(RealColor::Green),
            "blue" => Ok(RealColor::Blue),
            "red" => Ok(RealColor::Red),
            "cyan" => Ok(RealColor::Cyan),
            "magenta" => Ok(RealColor::Magenta),
            "grey" => Ok(RealColor::Grey),
            "white" => Ok(RealColor::White),
            "black" => Ok(RealColor::Black),
            _ => Err(E::custom(format!("Unknown RealColor: {}", value)))
        }
    }
}

impl Deserialize for RealColor {
    fn deserialize<D>(deserializer: D) -> Result<RealColor, D::Error>
        where D: Deserializer
    {
        deserializer.deserialize_str(RealColorVisitor)
    }
}

#[derive(Debug)]
struct Action {
    verbosity: Verbosity,
    directory: path::PathBuf,
    config: Config,
    formatter: Box<Formatter>,
}

#[derive(PartialEq, Eq, Clone)]
struct Attr {
    icon: String,
    color: ColorType,
}

fn get_file_attr(conf : &Config, suffix : &str) -> Attr {
    match conf.files.get(suffix) {
        Some(icon) => Attr { icon: icon.clone(), color: ColorType::RecognizedFile },
        None => Attr { icon: conf.files.get("file").unwrap().clone(), color: ColorType::UnrecognizedFile }
    }
}

fn get_file_attr_alias(conf : &Config, suffix : &str) -> Attr {
    match conf.file_aliases.get(suffix) {
        Some(alias) => get_file_attr(conf, alias),
        None => get_file_attr(conf, suffix)
    }
}

fn get_folder_attr(conf : &Config, name : &str) -> Attr {
    match conf.folders.get(name) {
        Some(icon) => Attr { icon: icon.clone(), color: ColorType::Dir },
        None => Attr { icon: conf.folders.get("folder").unwrap().clone(), color: ColorType::Dir }
    }
}

fn get_folder_attr_alias(conf : &Config, name : &str) -> Attr {
    match conf.folder_aliases.get(name) {
        Some(alias) => get_folder_attr(conf, alias),
        None => get_folder_attr(conf, name)
    }
}

fn filename_without_leading_dot(path : &path::Path) -> String {
    let mut file_name = path.file_name().unwrap().to_str().unwrap().to_string();
    file_name.remove(0);
    file_name
}

fn get_attr(config : &Config, path : &path::Path) -> Attr {
    if path.is_dir() {
        let file_name = path.file_name().unwrap().to_str().unwrap();
        return get_folder_attr_alias(config, file_name)
    } else {
        let filename_without_leading_dot = filename_without_leading_dot(path);
        let default = ffi::OsStr::new(&filename_without_leading_dot);
        let extension = path.extension().unwrap_or(default).to_str().unwrap();
        return get_file_attr_alias(config, extension)
    }
}

struct ColorWrapper(pub Box<color::Color>);

fn color_for(config : &Config, color : &ColorType) -> ColorWrapper {
   let boxed : Box<color::Color> = match config.colors.get(color).unwrap_or(&RealColor::Grey) {
       &RealColor::Yellow => Box::new(color::Yellow),
        &RealColor::Green => Box::new(color::Green),
        &RealColor::Blue => Box::new(color::Blue),
        &RealColor::Red => Box::new(color::Red),
        &RealColor::Cyan => Box::new(color::Cyan),
        &RealColor::Magenta => Box::new(color::Magenta),
        &RealColor::Grey => Box::new(color::AnsiValue::rgb(2,2,2)),
        &RealColor::White => Box::new(color::AnsiValue::rgb(0,0,0)),
        &RealColor::Black => Box::new(color::AnsiValue::rgb(5,5,5)),
   };
    ColorWrapper(boxed)
}

#[derive(Eq, Clone)]
struct Entry {
    path: path::PathBuf,
    attr: Attr,
}

impl Ord for Entry {
    fn cmp(&self, other: &Entry) -> Ordering {
        self.path.cmp(&other.path)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Entry) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Entry) -> bool {
        self.path == other.path
    }
}

impl color::Color for ColorWrapper {
    #[inline]
    fn write_fg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (*self.0).write_fg(f)
    }

    #[inline]
    fn write_bg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (*self.0).write_bg(f)
    }
}

struct EntryPrinterConfig {
    width : usize,
}

trait EntryPrinter: fmt::Debug {
    fn format(&self, &Config, &EntryPrinterConfig, &Entry) -> String;
    fn predict(&self, &Entry) -> usize;
}

#[derive(Debug)]
struct LongFormat {}
impl EntryPrinter for LongFormat {
    fn format(&self, config : &Config, ep_config : &EntryPrinterConfig, entry : &Entry) -> String {
            let name = entry.path.display();
            let width = ep_config.width - 2;
            format!("{icon} {color}{name:<width$}{reset}",
                     name = name,
                     icon = entry.attr.icon,
                     color = color::Fg(color_for(config, &entry.attr.color)),
                     reset = color::Fg(color::Reset),
                     width = width,
            )
    }

    fn predict(&self, entry : &Entry) -> usize {
        strlen(&format!("{}", entry.path.display())) + 2 // Icon + space
    }
}

#[derive(Debug)]
struct ShortFormat {}

fn short_name(l : &Entry) -> String {
    l.path.file_name().unwrap().to_str().unwrap().to_string()
}

impl EntryPrinter for ShortFormat {
    fn format(&self, config : &Config, ep_config : &EntryPrinterConfig, entry : &Entry) -> String {
        let name = short_name(entry);
        let width = ep_config.width - 2;
        format!("{icon}{color}{name:<width$}{reset}",
                name = name,
                icon = entry.attr.icon,
                color = color::Fg(color_for(config, &entry.attr.color)),
                reset = color::Fg(color::Reset),
                width = width,
        )
    }

    fn predict(&self, entry : &Entry) -> usize {
        strlen(&short_name(entry)) + 3
    }
}

fn strlen(s : &String) -> usize {
    s.graphemes(true).count() as usize
}

#[cfg(test)]
mod strlen_tests {
    use super::*;
    #[test]
    fn for_normal_string() {
        assert_eq!(6, strlen(&".local".to_string()))
    }

    #[test]
    fn for_string_with_icons() {
        assert_eq!(7, strlen(&".local".to_string()))
    }

    #[test]
    fn for_string_with_weird_stuff() {
        assert_eq!(7, strlen(&"a̐.local".to_string()))
    }

    #[test]
    fn for_string_with_icons_via_code() {
        assert_eq!(7, strlen(&format!("{}.local", "\u{f115}")))
    }

    // NOTE: Nope. Not gonna work.
    // #[test]
    // fn for_string_with_color() {
    //     assert_eq!(6, strlen(&format!("{color}.local{reset}", color = color::Fg(color::Red), reset = color::Fg(color::Reset))))
    // }
}

type Output = Vec<Vec<String>>;

trait Formatter: fmt::Debug {
    fn format(&self, &Config, Vec<Entry>) -> Output;
}

fn as_rows<T : Clone>(names : &Vec<T>, row_cap : usize) -> Vec<Vec<T>> {
    let mut rows = Vec::with_capacity(names.len() / row_cap + 1);
    let mut row = Vec::with_capacity(row_cap);
    for (i, out) in names.iter().enumerate() {
        row.push(out.clone());
        if i % row_cap == row_cap - 1 {
            rows.push(row);
            row = Vec::new();
        }
    }
    rows
}

// NOTE: Assumes out has same-sized rows
fn is_valid(out : Vec<Vec<usize>>, max_width : usize) -> bool {
    let mut col_widths = vec![0; out[0].len()];
    for r in out {
        for (i, s) in r.iter().enumerate() {
            col_widths[i] = max(col_widths[i], *s);
        }
    }
    let mut width = 0;
    for c in col_widths { width += c }
    return width < max_width
}

#[cfg(test)]
mod is_valid_tests {
    use super::*;
    #[test]
    fn small_case() {
        assert_eq!(false, is_valid(vec![vec![1,2], vec![2,1]], 2))
    }
}

fn is_valid_as_rows(config: &Config, names : &Vec<Entry>, row_cap : usize) -> bool {
    is_valid(as_rows(&names.iter().map(|e| config.printer.predict(e)).collect(), row_cap), config.max_width)
}

fn format_as_rows(config : &Config, names : &Vec<Entry>, row_cap : usize) -> Output {
    let rows = as_rows(names, row_cap);
    let mut col_widths = vec![0; rows[0].len()];
    for r in &rows {
        for (i, s) in r.iter().enumerate() {
            let predicted = config.printer.predict(s);
            if predicted > col_widths[i] {
                col_widths[i] = predicted
            }
        }
    }
    let ep_configs : Vec<EntryPrinterConfig> = col_widths.iter().map(|width| EntryPrinterConfig{width: *width}).collect();
    let mut out = Vec::with_capacity(names.len());
    for r in rows {
        for (i, s) in r.iter().enumerate() {
            out.push(config.printer.format(config, &ep_configs[i], s));
        }
    }
    as_rows(&out, row_cap)
}

fn max_width(config : &Config, names : &Vec<Entry>) -> usize {
    let mut width = 0;
    for l in names {
        let cwidth = config.printer.predict(l);
        if cwidth > width {
            width = cwidth;
        }
    }
    width
}

const MIN_FORMAT_ENTRY_LENGTH : usize = 5;

#[derive(Debug)]
struct PlanningFormatter {}
impl Formatter for PlanningFormatter {
    fn format(&self, config : &Config, names : Vec<Entry>) -> Output {
        let width = max_width(config, &names);
        let min_rows = (config.max_width / (width + 1)) as i64;
        let max_rows = (config.max_width / MIN_FORMAT_ENTRY_LENGTH) as i64;
        for row_cap in range_step(max_rows, min_rows, -1) {
            if is_valid_as_rows(config, &names, row_cap as usize) {
                return format_as_rows(config, &names, row_cap as usize)
            }
        }
        format_as_rows(config, &names, min_rows as usize)
    }
}

#[derive(Debug)]
struct NaiveFormatter {}
impl Formatter for NaiveFormatter {
    fn format(&self, config : &Config, names : Vec<Entry>) -> Output {
        let width = max_width(config, &names) + 2;
        let rows = config.max_width / width;
        format_as_rows(config, &names, rows)
    }
}

fn run(action : Action) {
    if action.verbosity != Verbosity::Quiet {
        println!("Looking at {}", action.directory.display());

    }
    let dirs = fs::read_dir(action.directory).unwrap();
    let config = action.config;
    let ls = dirs.map(|dir| {
        let path = dir.unwrap().path();
        Entry { path: path.clone(), attr: get_attr(&config, &path) }
    }).collect();
    let rows = action.formatter.format(&config, ls);
    for items in rows {
        for item in items {
            print!("{}", item);
        }
        println!("");
    }
}

fn main() {
    let matches = App::new("ColorLs")
        .version("0.1.0")
        .author("scoiatael <czapl.luk+git@gmail.com>")
        .about("List information about the FILEs (the current directory by default).")
        .arg(Arg::with_name("long")
             .long("long")
             .short("l")
             .help("Prints using long format"))
        .arg(Arg::with_name("naive")
             .long("naive")
             .short("n")
             .help("Prints using naive formatter"))
        .arg(Arg::with_name("v")
             .short("v")
             .multiple(true)
             .help("Sets the level of verbosity"))
        .arg(Arg::with_name("FILE")
             .required(false)
             .index(1))
        .get_matches();

    let verbosity = match matches.occurrences_of("v") {
        0 => Verbosity::Quiet,
        1 => Verbosity::Warn,
        2 | _ =>  Verbosity::Debug,
    };
    let formatter : Box<Formatter> = match matches.occurrences_of("naive") {
        0 => Box::new(PlanningFormatter{}),
        1 | _ => Box::new(NaiveFormatter{}),
    };
    let printer : Box<EntryPrinter> = match matches.occurrences_of("long") {
        0 => Box::new(ShortFormat{}),
        1 | _ =>  Box::new(LongFormat{}),
    };

    let file_icons = serde_yaml::from_str(include_str!("default_config/files.yaml")).unwrap();
    let folder_icons = serde_yaml::from_str(include_str!("default_config/folders.yaml")).unwrap();
    let file_aliases = serde_yaml::from_str(include_str!("default_config/file_aliases.yaml")).unwrap();
    let folder_aliases = serde_yaml::from_str(include_str!("default_config/folder_aliases.yaml")).unwrap();
    let colors = serde_yaml::from_str(include_str!("default_config/dark_colors.yaml")).unwrap();
    let cdir_path = env::current_dir().unwrap();
    let dir = matches.value_of("FILE").unwrap_or_else(|| cdir_path.to_str().unwrap());
    let path = path::PathBuf::from(dir);
    let action = Action {
        verbosity: verbosity,
        directory: path,
        config: Config {
            files: file_icons,
            file_aliases: file_aliases,
            folders: folder_icons,
            folder_aliases: folder_aliases,
            colors: colors,
            max_width: terminal_size().unwrap().0 as usize,
            printer: printer,
        },
        formatter: formatter,
    };

    if verbosity == Verbosity::Debug {
        println!("{:?}", action);

    }
    run(action);
}
