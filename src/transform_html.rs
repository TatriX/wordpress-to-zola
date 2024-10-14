use html2md::NodeData;
use html5ever::QualName;
use html5ever::{tendril::TendrilSink, tree_builder::TreeBuilderOpts, ParseOpts};
use markup5ever_rcdom::Node;
use markup5ever_rcdom::RcDom;
use markup5ever_rcdom::SerializableHandle;
use regex::Regex;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

/// Wordpress does some transformations on its HTML before it displays it.
/// Attempt to recreate them here.
pub fn transform_html(content: &str) -> String {
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let dom = html5ever::parse_document(RcDom::default(), opts).one(content);

    let html = find_child_element(dom.document.clone(), "html");
    let body = find_child_element(html, "body");

    let newlines = Regex::new(r"\n\n+").unwrap();

    let mut i = 0;
    let mut texts: Vec<(isize, String)> = Vec::new();
    for child in body.children.borrow().iter() {
        if let NodeData::Text { contents } = child.data.borrow() {
            let text = contents.borrow().deref().deref().to_owned();
            if newlines.is_match(&text) {
                texts.push((i, text));
            }
        }
        i += 1;
    }

    let mut changed = false;
    let mut offset: isize = 0;
    for (i, text) in texts {
        changed = true;

        body.children.borrow_mut().remove((i + offset) as usize);
        offset -= 1;

        for chunk in itertools::intersperse(newlines.split(&text), &"\n\n") {
            if chunk == "\n\n" {
                body.children
                    .borrow_mut()
                    .insert((i + offset + 1) as usize, p_node());
                offset += 1;
            } else {
                body.children
                    .borrow_mut()
                    .insert((i + offset + 1) as usize, text_node(chunk));
                offset += 1;
            }
        }
    }

    if changed {
        let mut ret = Vec::new();
        let ser: SerializableHandle = body.clone().into();
        html5ever::serialize(&mut ret, &ser, Default::default())
            .expect("Failed to serialize modified HTML");
        String::from_utf8_lossy(&ret).into_owned()
    } else {
        content.to_owned()
    }
}

fn text_node(text: &str) -> Rc<Node> {
    Node::new(NodeData::Text {
        contents: RefCell::new(text.into()),
    })
}

fn p_node() -> Rc<Node> {
    Node::new(NodeData::Element {
        name: QualName::new(None, "".into(), "p".into()),
        attrs: RefCell::new(Vec::new()),
        template_contents: RefCell::new(None),
        mathml_annotation_xml_integration_point: false,
    })
}

fn find_child_element(parent: Rc<Node>, tag: &str) -> Rc<Node> {
    // Find the nth child
    let children = parent.children.borrow();
    for child in children.iter() {
        if let NodeData::Element { name, .. } = child.data.borrow() {
            if name.local.eq_str_ignore_ascii_case(tag) {
                return child.clone();
            }
        }
    }
    panic!("Unable to find a {} element", tag);
}

#[cfg(test)]
mod tests {
    use crate::transform_html::transform_html;

    #[test]
    fn no_newlines_means_no_change() {
        assert_eq!(transform_html("ab"), "ab");
        assert_eq!(transform_html("<b>A</b>B<b>C</b>"), "<b>A</b>B<b>C</b>");
    }

    #[test]
    fn one_new_line_is_preserved() {
        assert_eq!(transform_html("a\nb"), "a\nb");
        assert_eq!(transform_html("a\n\nb\nc"), "a<p></p>b\nc");
    }

    #[test]
    fn gaps_yield_separate_paragraphs() {
        assert_eq!(transform_html("a\n\nb"), "a<p></p>b");
    }

    #[test]
    fn long_gaps_are_the_same_as_short_ones() {
        assert_eq!(transform_html("a\n\n\n\n\n\nb"), "a<p></p>b");
    }

    #[test]
    fn leading_and_trailing_newlines_are_ignored() {
        assert_eq!(transform_html("a\n\n"), "a<p></p>");
        assert_eq!(transform_html("\n\na"), "\n\na");
        assert_eq!(transform_html("a\n\nb\n\n"), "a<p></p>b<p></p>");
        assert_eq!(transform_html("\n\na\n\nb\n\n"), "a<p></p>b<p></p>");
    }

    #[test]
    fn multiple_gaps_become_paras() {
        assert_eq!(transform_html("a\n\nb\n\nc"), "a<p></p>b<p></p>c");
    }

    #[test]
    fn tags_containing_gaps_are_preserved_as_is() {
        assert_eq!(transform_html("<b>a\n\nb\n\nc</b>"), "<b>a\n\nb\n\nc</b>");
        assert_eq!(
            transform_html("<b>a\n\nb\n\nc</b>\n\nd"),
            "<b>a\n\nb\n\nc</b><p></p>d"
        );
        assert_eq!(
            transform_html("a<b>b\n\nb\n\nb</b>\n\nc"),
            "a<b>b\n\nb\n\nb</b><p></p>c"
        );
    }

    #[test]
    fn text_followed_by_tag_is_untouched() {
        assert_eq!(transform_html("a\n\nb<tt>c</tt>"), "a<p></p>b<tt>c</tt>");
    }

    #[test]
    fn trailing_newline_after_tags_is_preserved() {
        assert_eq!(
            transform_html("<tt>a</tt>\n\n<tt>b</tt>\n"),
            "<tt>a</tt><p></p><tt>b</tt>\n"
        );
    }

    #[test]
    fn comments_are_ok() {
        assert_eq!(transform_html("a<!--  -->"), "a<!--  -->");
        assert_eq!(transform_html("a\n\nb<!--  -->"), "a<p></p>b<!--  -->");
        assert_eq!(transform_html("<!--  -->"), "<!--  -->");
        assert_eq!(transform_html("<!-- a -->"), "<!-- a -->");
        assert_eq!(transform_html("<p>a</p><!--  -->"), "<p>a</p><!--  -->");
        assert_eq!(transform_html("<p>a<!--  -->b</p>"), "<p>a<!--  -->b</p>");
        assert_eq!(transform_html("<p>a<!-- b -->c</p>"), "<p>a<!-- b -->c</p>");
    }

    #[test]
    fn leading_comments_are_skipped() {
        // For some reason, leading comments are moved out to the document level by html5ever.
        // This slightly incorrect, but hopefully unproblematic behaviour is documented here:
        assert_eq!(transform_html("<!--  -->b\n\nc"), "b<p></p>c");

        // This only happens when we actually change the HTML, so when there are no bare text nodes
        // the text is unchanged.
        assert_eq!(transform_html("<!--  -->b"), "<!--  -->b");
        assert_eq!(transform_html("<!--  --><p>b</p>"), "<!--  --><p>b</p>");
    }
}
