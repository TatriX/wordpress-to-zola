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
use std::env::args;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::{Read, Result, Write};
use std::path::{Path, PathBuf};

/// Paginate section by this number of posts.
/// TODO: make configurable
const PAGINATE_BY: usize = 5;

fn main() -> Result<()> {
    env_logger::init();

    if let [input, output] = args().skip(1).take(2).collect::<Vec<_>>().as_slice() {
        let fs = RealFs {};

        convert(input.into(), output.into(), &fs)?;
    } else {
        eprintln!("Usage: wordpress-to-zola ./input.xml ./output-dir");
    }
    Ok(())
}

/// Read xml from `input_file` and create `zola` content directory in
/// `output_dir`.
fn convert(input_file: PathBuf, output_dir: PathBuf, fs: &impl Fs) -> Result<()> {
    let file = fs.open(&input_file)?;
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
            _ => continue,        // skip everything else
        }
        match item.post_type {
            PostType::Post => {
                let path = output_dir.join(generate_path(&base_url, &item.link));
                info!("Post [{:?}] {} -> {:?}", item.status, item.title, &path);

                let section = path.parent().expect("no parent in filename");
                // ensure all directories are in place
                debug!("Creating directory {:?}", section);
                fs.create_dir_all(&path.parent().expect("no parent in filename"))?;

                // if it's the first time we see this section, create section file
                if sections.insert(section.to_owned()) {
                    fs.create_section(section)?;
                }

                let date =
                    DateTime::parse_from_rfc2822(&item.pub_date).expect("cannot parse pubDate");

                let markdown = parse_html(item.content());
                let markdown = markdown.replace("[]()", "");
                debug!("{}", markdown);

                fs.create_page(&path, &item.title.replace('"', "\\\""), date, &markdown)?;
            }
            PostType::Attachment => debug!("Ignoring attachment {}", item.title),
            _ => debug!("Ignoring unknown post type {}", item.title),
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
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Publish,
    Draft,
    Inherit,
    Private,
}

trait Fs {
    fn open(&self, path: &PathBuf) -> Result<impl Read>;

    fn create_dir_all<P>(&self, path: P) -> Result<()>
    where
        P: AsRef<Path>;

    fn create_page(
        &self,
        path: &Path,
        title: &str,
        date: DateTime<FixedOffset>,
        markdown: &str,
    ) -> Result<()>;

    fn create_section(&self, section: &Path) -> Result<()>;
}

struct RealFs {}

impl Fs for RealFs {
    fn open(&self, path: &PathBuf) -> Result<impl Read> {
        File::open(path)
    }

    fn create_dir_all<P>(&self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        create_dir_all(path)
    }

    /// Create post file
    fn create_page(
        &self,
        path: &Path,
        title: &str,
        date: DateTime<FixedOffset>,
        markdown: &str,
    ) -> Result<()> {
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

    /// Create section `_index.md` file.
    fn create_section(&self, section: &Path) -> Result<()> {
        let mut file = File::create(section.join("_index.md"))?;
        writeln!(file, "+++")?;
        writeln!(file, "transparent = true")?; // show pages from this section in index.html
        writeln!(file, "sort_by = \"date\"")?;
        writeln!(file, "paginate_by = {}", PAGINATE_BY)?;
        writeln!(file, "+++")?;
        Ok(())
    }
}

/// Generate path for an item by splicing base url from the link.
fn generate_path(base_url: &str, link: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}.md",
        link.trim_start_matches(&base_url).trim_matches('/')
    ))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::{convert, Fs};

    struct FakeFs {
        input: String,
        calls: RefCell<Vec<String>>,
    }

    impl FakeFs {
        fn new(input: &str) -> Self {
            Self {
                input: input.to_owned(),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Fs for FakeFs {
        fn open(&self, _path: &std::path::PathBuf) -> std::io::Result<impl std::io::Read> {
            Ok(self.input.as_bytes())
        }

        fn create_dir_all<P>(&self, path: P) -> std::io::Result<()>
        where
            P: AsRef<std::path::Path>,
        {
            self.calls
                .borrow_mut()
                .push(format!("create_dir_all({:?})", path.as_ref()));
            Ok(())
        }

        fn create_page(
            &self,
            path: &std::path::Path,
            title: &str,
            date: chrono::DateTime<chrono::FixedOffset>,
            markdown: &str,
        ) -> std::io::Result<()> {
            self.calls.borrow_mut().push(format!(
                "create_page({:?}, {}, {}, {})",
                path, title, date, markdown
            ));
            Ok(())
        }

        fn create_section(&self, section: &std::path::Path) -> std::io::Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("create_section({:?})", section));
            Ok(())
        }
    }

    #[test]
    fn normal_posts_are_converted() {
        // Given a WP export with a post in it
        let input = r#"<?xml version="1.0" encoding="UTF-8" ?>
            <rss version="2.0"
                xmlns:content="http://purl.org/rss/1.0/modules/content/"
                xmlns:wp="http://wordpress.org/export/1.2/"
            >
            <channel>
                <title>Blog</title>
                <wp:base_site_url>https://example.com</wp:base_site_url>
                <item>
                    <title>Post 1</title>
                    <pubDate>Mon, 01 Sep 2008 21:02:27 +0000</pubDate>
                    <description></description>
                    <link>http://example.com/post1</link>
                    <content:encoded><![CDATA[]]></content:encoded>
                    <wp:post_type><![CDATA[post]]></wp:post_type>
                    <wp:status><![CDATA[publish]]></wp:status>
                </item>
            </channel>
        </rss>
        "#;

        // When we convert it
        let fs = FakeFs::new(input);
        convert("".into(), "output".into(), &fs).unwrap();

        // Then we create a post and section
        assert_eq!(
            fs.calls(),
            &[
                "create_dir_all(\"output/http://example.com\")",
                "create_section(\"output/http://example.com\")",
                "create_page(\
                    \"output/http://example.com/post1.md\", \
                    Post 1, \
                    2008-09-01 21:02:27 +00:00, \
                )",
            ]
        );
    }

    #[test]
    fn unknown_post_types_are_ignored() {
        // Given a blog item wpcode post_tyoe
        let input = r#"<?xml version="1.0" encoding="UTF-8" ?>
            <rss version="2.0"
                xmlns:content="http://purl.org/rss/1.0/modules/content/"
                xmlns:wp="http://wordpress.org/export/1.2/"
            >
            <channel>
                <title>Blog</title>
                <wp:base_site_url>https://example.com</wp:base_site_url>
                <item>
                    <title>Post 1</title>
                    <pubDate>Mon, 01 Sep 2008 21:02:27 +0000</pubDate>
                    <description></description>
                    <link>http://example.com/post1</link>
                    <content:encoded><![CDATA[]]></content:encoded>
                    <wp:post_type><![CDATA[wpcode]]></wp:post_type>
                    <wp:status><![CDATA[publish]]></wp:status>
                </item>
            </channel>
        </rss>
        "#;

        // When we convert it
        let fs = FakeFs::new(input);
        convert("".into(), "output".into(), &fs).unwrap();

        // Then nothing was generated
        assert!(fs.calls().is_empty());
    }

    #[test]
    fn quotes_in_titles_are_escaped() {
        // Given a blog item with quotes in its title
        let input = r#"<?xml version="1.0" encoding="UTF-8" ?>
            <rss version="2.0"
                xmlns:content="http://purl.org/rss/1.0/modules/content/"
                xmlns:wp="http://wordpress.org/export/1.2/"
            >
            <channel>
                <title>Blog</title>
                <wp:base_site_url>https://example.com</wp:base_site_url>
                <item>
                    <title>Post "1"</title>
                    <pubDate>Mon, 01 Sep 2008 21:02:27 +0000</pubDate>
                    <description></description>
                    <link>http://example.com/post1</link>
                    <content:encoded><![CDATA[]]></content:encoded>
                    <wp:post_type><![CDATA[post]]></wp:post_type>
                    <wp:status><![CDATA[publish]]></wp:status>
                </item>
            </channel>
        </rss>
        "#;

        // When we convert it
        let fs = FakeFs::new(input);
        convert("".into(), "output".into(), &fs).unwrap();

        // Then the created post escapes the quotes in the title
        assert_eq!(
            fs.calls(),
            &[
                "create_dir_all(\"output/http://example.com\")",
                "create_section(\"output/http://example.com\")",
                "create_page(\
                    \"output/http://example.com/post1.md\", \
                    Post \\\"1\\\", \
                    2008-09-01 21:02:27 +00:00, \
                )",
            ]
        );
    }

    #[test]
    fn empty_links_are_removed() {
        // Given a blog item with empty links
        let input = r#"<?xml version="1.0" encoding="UTF-8" ?>
            <rss version="2.0"
                xmlns:content="http://purl.org/rss/1.0/modules/content/"
                xmlns:wp="http://wordpress.org/export/1.2/"
            >
            <channel>
                <title>Blog</title>
                <wp:base_site_url>https://example.com</wp:base_site_url>
                <item>
                    <title>Post "1"</title>
                    <pubDate>Mon, 01 Sep 2008 21:02:27 +0000</pubDate>
                    <description></description>
                    <link>http://example.com/post1</link>
                    <content:encoded><![CDATA[Foo []() Bar]]></content:encoded>
                    <wp:post_type><![CDATA[post]]></wp:post_type>
                    <wp:status><![CDATA[publish]]></wp:status>
                </item>
            </channel>
        </rss>
        "#;

        // When we convert it
        let fs = FakeFs::new(input);
        convert("".into(), "output".into(), &fs).unwrap();

        // Then the created post escapes the quotes in the title
        assert_eq!(
            fs.calls(),
            &[
                "create_dir_all(\"output/http://example.com\")",
                "create_section(\"output/http://example.com\")",
                "create_page(\
                    \"output/http://example.com/post1.md\", \
                    Post \\\"1\\\", \
                    2008-09-01 21:02:27 +00:00, \
                    Foo  Bar\
                )",
            ]
        );
    }
}
