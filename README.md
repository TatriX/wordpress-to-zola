# wordpress-to-zola
 Wordress to Zola converter.

## What & Why?

This is a small tool for generating sections and pages for [zola][]
from wordress XML.  If you want to move your blog from wordress to
zola, this tool will do that for you.

## How do I use it?

First you should go to your wordpress's `/wp-admin/export.php` and
download XML file.  Then you run `cargo run -- ./input.xml
./output-dir` which will make section directories with posts inside.
