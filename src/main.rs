//! # wordpress-to-zola
//! Wordress to Zola converter.
//!
//! ## What & Why?
//!
//! This is a small tool for generating sections and pages for
//! [zola][] from wordress XML.  If you want to move your blog from
//! wordress to zola, this tool will do that for you.
//!
//! ## How do I use it?
//!
//! First you should go to your wordpress's `/wp-admin/export.php` and
//! download XML file.  Then you run `cargo run -- input.xml` and it
//! will produce a `content` directory will all the pages and
//! sections.
//!
//! ## How does it work?
//!
//! TODO: document
//! TODO: generate config.toml?
//!
//! ## Debugging
//! One may want to set logging level to debug to see more details.
//! ```
//! export RUST_LOG=wordpress_to_zola=debug
//! cargo run
//! ```
//!
//! [zola][https://www.getzola.org/]

use chrono::{DateTime, FixedOffset};
use html2md::parse_html;
use log::*;
use serde::Deserialize;
use serde_xml_rs::from_reader;
use std::collections::HashSet;
use std::fs::create_dir_all;
use std::fs::File;
use std::env::args;
use std::io::{Result, Write};
use std::path::{Path, PathBuf};

/// Paginate section by this number of posts.
/// TODO: make configurable
const PAGINATE_BY: usize = 5;

fn main() -> Result<()> {
    env_logger::init();

    if let [input, output] = args().skip(1).take(2).collect::<Vec<_>>().as_slice() {
        convert(input.into(), output.into())?;
    } else {
        eprintln!("Usage: wordpress-to-zola ./input.xml ./output-dir");
    }
    Ok(())
}

/// Read xml from `input_file` and create `zola` content directory in
/// `output_dir`.
fn convert(input_file: PathBuf, output_dir: PathBuf) -> Result<()> {
    let file = File::open(input_file)?;
    let rss: Rss = from_reader(file).expect("cannot parse xml");

    // We want to strip `base_url` from posts url later on to get a
    // nice filename for a post.
    let base_url = rss.channel.base_site_url;

    // We will make `_index.md` for every top level section we will
    // find. This set is used to only do that once per section.
    let mut sections = HashSet::new();

    for item in rss.channel.item {
        match item.status {
            Status::Publish => {} // take only published posts
            _ => continue, // skip everything else
        }
        match item.post_type {
            PostType::Post => {
                let path = output_dir.join(generate_path(&base_url, &item.link));
                info!("Post [{:?}] {} -> {:?}", item.status, item.title, &path);

                let section = path.parent().expect("no parent in filename");
                // ensure all directories are in place
                debug!("Creating directory {:?}", section);
                create_dir_all(&path.parent().expect("no parent in filename"))?;

                // if it's the first time we see this section, create section file
                if sections.insert(section.to_owned()) {
                    create_section(section)?;
                }

                let date = DateTime::parse_from_rfc2822(&item.pub_date)
                    .expect("cannot parse pubDate");

                let markdown = parse_html(item.content());
                debug!("{}", markdown);

                create_page(&path, &item.title, date, &markdown)?;

            },
            _ => debug!("Ignoring attachment {}", item.title),
        }
    }
    Ok(())
}


/// Top level wrapper
#[derive(Debug, Deserialize)]
struct Rss {
    channel: Channel,
}

/// Main wrapper
#[derive(Debug, Deserialize)]
struct Channel {
    base_site_url: String,
    item: Vec<Item>,
}

/// Item can be either Post or Attachment
#[derive(Debug, Deserialize)]
struct Item {
    title: String,
    link: String,
    #[serde(rename = "pubDate")]
    pub_date: String,
    post_type: PostType,
    encoded: Vec<String>,
    status: Status,
}

impl Item {
    /// Helper method to workaround serde-xml inability to work with
    /// fields containing colons.
    ///
    /// See https://github.com/RReverser/serde-xml-rs/issues/64
    fn content(&self) -> &str {
        &self.encoded[0]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PostType {
    Attachment,
    Post,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Publish,
    Draft,
    Inherit,
    Private,
}

/// Create section `_index.md` file.
fn create_section(section: &Path) -> Result<()> {
    let mut file = File::create(section.join("_index.md"))?;
    writeln!(file, "+++")?;
    writeln!(file, "transparent = true")?; // show pages from this section in index.html
    writeln!(file, "sort_by = \"date\"")?;
    writeln!(file, "paginate_by = {}", PAGINATE_BY)?;
    writeln!(file, "+++")?;
    Ok(())
}

/// Create post file
fn create_page(path: &Path, title: &str, date: DateTime<FixedOffset>, markdown: &str) -> Result<()> {
    let mut file = File::create(path)?;
    // write front-matter
    writeln!(file, "+++")?;
    writeln!(file, "title = \"{}\"", title)?;
    writeln!(file, "date = {}", date.to_rfc3339())?;
    writeln!(file, "+++")?;
    // and content
    writeln!(file, "{}", markdown)?;
    Ok(())
}

/// Generate path for an item by splicing base url from the link.
fn generate_path(base_url: &str, link: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}.md",
        link.trim_start_matches(&base_url).trim_matches('/')
    ))
}
