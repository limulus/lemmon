//! Fixtures shared by this crate's unit tests: the hosts these tests compile against,
//! the declarations those hosts read, the calls that compile one file into a [`Unit`]
//! or compile and link a whole set of them, and [`shown`], which prints diagnostics for
//! comparison. A diagnostic's expected span comes from `lemmon_syntax::spans` instead,
//! which every stage's tests share.

use crate::SourceFile;
use crate::bytecode::Module;
use crate::compile::{compile_unit, prepare};
use crate::diagnostics::{Compiled, Diagnostic};
use crate::exports::ExportIndex;
use crate::host::{Ambient, Host};
use crate::link::link;
use crate::unit::Unit;

/// The host a test compiles against unless it needs another: the three names the
/// standard shell binds, none of them typed.
pub(crate) fn untyped_host() -> Host {
    Host::untyped(["site", "page", "build"])
}

/// A host shaped like the standard shell's, reading `site` from `site.lem`: `page` is a
/// declared `Page`, and every page renders through `layout`. It names no url field;
/// [`addressed_host`] is the same host with the url check armed.
pub(crate) fn shell_host() -> Host {
    Host::new()
        .bind("page", Ambient::named("Page"))
        .bind("site", Ambient::bindings("site.lem"))
        .documents("page")
        .rendered_through("layout")
}

/// [`shell_host`] with its pages addressed by their `url` field (D43) — the host the link
/// step's tests use, so that a page's url is checked unless a test says otherwise.
pub(crate) fn addressed_host() -> Host {
    shell_host().addressed_by("url")
}

/// `shell_host`'s document type, as a host really passes it: an ordinary file, compiled
/// with the program.
pub(crate) const AMBIENT: &str = "<lem-type Page>\n  url: url\n</lem-type>";

/// A page schema extending `Page` with a title and a layout field.
pub(crate) const POST: &str = "<lem-type Post extends Page>\n  title: string\n  \
     layout: (Post, html): html\n</lem-type>";

/// Compiles the file at `path` into a unit against an index of every given file's
/// exports.
pub(crate) fn unit_of(path: &str, index_of: &[(&str, &str)]) -> Unit {
    let prepared: Vec<_> = index_of
        .iter()
        .map(|(path, text)| {
            prepare(&SourceFile::new(*path, *text), &untyped_host()).unwrap()
        })
        .collect();
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(p.path(), p.exports());
        index.offer_types(p.path(), p.types());
    }
    let this = prepared
        .iter()
        .find(|p| p.path() == path)
        .expect("the file compiles against an index that includes it");
    compile_unit(this, &index, &untyped_host()).expect_output()
}

/// Compiles the file at `path` against `host`, and against an index of every given
/// file's exports.
pub(crate) fn unit_against(host: &Host, path: &str, files: &[(&str, &str)]) -> Unit {
    let prepared: Vec<_> = files
        .iter()
        .map(|(path, text)| prepare(&SourceFile::new(*path, *text), host).unwrap())
        .collect();
    let mut index = ExportIndex::new();
    for p in &prepared {
        index.offer(p.path(), p.exports());
        index.offer_types(p.path(), p.types());
    }
    let this = prepared
        .iter()
        .find(|p| p.path() == path)
        .expect("the file compiles against an index that includes it");
    compile_unit(this, &index, host).expect_output()
}

/// Every file compiled against `host` and linked: the module, or what stopped it.
pub(crate) fn linked(host: &Host, files: &[(&str, &str)]) -> Compiled<Module> {
    let units: Vec<Unit> = files
        .iter()
        .map(|(path, _)| unit_against(host, path, files))
        .collect();
    link(&units, &ExportIndex::of(&units), host)
}

/// Every diagnostic as the author is shown it.
pub(crate) fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(ToString::to_string).collect()
}
