//! The ambient environment a host declares (D30, `docs/language.md` §Ambient types).
//!
//! The core knows no type or value by name. A host — the shell, the LSP, a playground —
//! says which names every template may read and what each one's type is, and the types
//! themselves are ordinary declarations in files it compiles with the program, so every
//! field has a span and the rules that check the site's records check what the host
//! sends. A [`Host`] is that declaration as the compiler reads it: names to bindings,
//! plus the two names a host that renders documents adds — which ambient value a file's
//! record retypes, and the field a page is rendered through.
//!
//! A host that names nothing compiles a program with no ambient values: `page` is then
//! an undefined name like any other, not a reserved one.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lemmon_syntax::nearest;

use crate::document::Grammars;
use crate::types::Type;

/// What a host binds one ambient name to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ambient {
    /// Named without a type: `any`, checked by the VM at run time. A host is free to
    /// declare nothing about a value, which is how the Phase 0 spike renders.
    Untyped,
    /// A type the program declares.
    Named(Rc<str>),
    /// The record of a file's top-level bindings — `site` from `site.lem`. The
    /// declaration is synthesized where that file is prepared, and named by its path,
    /// which no `lem-type` can collide with.
    Bindings(Rc<str>),
}

impl Ambient {
    /// Bound to the declared type `name`.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self::Named(Rc::from(name))
    }

    /// Bound to the record of the top-level bindings of the file at `path`.
    #[must_use]
    pub fn bindings(path: &str) -> Self {
        Self::Bindings(Rc::from(path))
    }

    /// The type a value bound this way has.
    #[must_use]
    pub fn ty(&self) -> Type {
        match self {
            Self::Untyped => Type::Any,
            Self::Named(name) | Self::Bindings(name) => Type::Named(name.clone()),
        }
    }
}

/// Why `path` is no asset of `host`: the nearest one it declared, or that it declared
/// none at all.
///
/// A message rather than a fact, and here for the reason [`no_field`](crate::decls) is
/// in `decls`: refusing a path means knowing every path the manifest holds, so the one
/// refusal the compiler and the loader share is built where that knowledge lives. The
/// two must agree — a path is one mistake whether a template named it or a document's
/// front matter did.
#[must_use]
pub fn no_asset(host: &Host, path: &str) -> String {
    let mut out = format!("the manifest has no asset `{path}`");
    if let Some(near) = nearest(path, host.asset_paths()) {
        out.push_str(&format!("; did you mean `{near}`?"));
    } else if host.asset_paths().next().is_none() {
        out.push_str("; the host declares none");
    }
    out
}

/// Everything a host declares about the environment its programs compile in — and,
/// in the view [`for_file`](Host::for_file) makes, what one file of them sees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    /// Every ambient name, in the order the host gave them. A `Vec` rather than a map:
    /// a host declares a handful of names, and the order is what makes a diagnostic
    /// listing them deterministic.
    ambient: Vec<(Rc<str>, Ambient)>,
    /// The ambient value a file's `record` retypes — `page` in the standard shell.
    /// A host that renders no documents names none.
    document: Option<Rc<str>>,
    /// The field a page is rendered through — `layout` in the standard shell — required
    /// on every record that extends the document value's type.
    entry: Option<Rc<str>>,
    /// What the document value is in the file this is a view of: that file's record, or
    /// the unknown type for a document the host matched to nothing. Never set in the
    /// declaration itself, which is why the type a page schema extends stays the
    /// host's however a file was compiled.
    document_binding: Option<Ambient>,
    /// Every asset the host's build produced, as `(logical path, the URL it shipped
    /// at)`, in the order the host listed them.
    ///
    /// The value is the finished URL and not a hash, because the core does no path
    /// arithmetic: where an asset ends up is the shell's question, and a core that
    /// assembled a URL from parts would be deciding it (D38).
    ///
    /// It rides on the declaration rather than arriving with the data because
    /// `asset("…")` is answered while compiling (D10): the URL is folded into the
    /// bytecode, and the entry it was folded from is a compile-time fact the link step
    /// compares (D25, invariant 11). Being a field of the declaration is also what
    /// makes a manifest change a *declaration* change to the derived `PartialEq`, which
    /// is what the resident `Core` compares to decide whether a recompile is needed.
    assets: Vec<(Rc<str>, Rc<str>)>,
    /// The grammars a document's code is highlighted with, as `.sublime-syntax` text
    /// (D40); `None` for a host that asks for no highlighting, which leaves every code
    /// block as comrak writes it. On the declaration for the manifest's reason: prose
    /// renders while compiling, so the grammars are consumed there, and a document
    /// records the set it was highlighted against.
    grammars: Option<Grammars>,
    /// What the host's handlers claim (D42), by mark name; `None` for a host that
    /// declares no handlers at all — a playground, an editor — which is silent about
    /// every name rather than warning at each. On the declaration for the manifest's
    /// reason: a name is checked while a file compiles, and the file records what the
    /// claims said.
    marks: Option<BTreeMap<Rc<str>, Claim>>,
}

/// What a host's handlers claim for one mark name (D42): the keys its producers cover,
/// the record a mark of the name carries, and which field of that record holds a mark's
/// key.
///
/// Several handlers may claim one name — a producer and a decorator, two producers on
/// disjoint keys — so this is the merge of what they said: keys are unioned, while the
/// payload and the key field are the name's own and have to agree. Two producers on one
/// key, and two claims disagreeing about a payload, are the host's to refuse before a
/// declaration is built from them (D42; the plugin-load error is ∆PgF's).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Claim {
    /// The keys the name's producers cover, sorted — what a did-you-mean at a key is
    /// drawn from, and what makes the listing deterministic.
    keys: BTreeSet<Rc<str>>,
    /// Whether any claim for this name listed keys at all. A producer that lists keys
    /// covers those and no others; a claim that lists none covers whatever comes, which
    /// is every decorator and the shell's own `styles` bundler. The two are not the same
    /// as an empty list, so this is a flag rather than `keys.is_empty()`.
    keyed: bool,
    /// The record a mark of this name carries, where the handlers take a payload.
    payload: Option<Rc<str>>,
    /// The field of that record whose value is a mark's key — `language` on `code`.
    /// Named by the claim rather than found by convention, since the plugin that
    /// declares the record is the one that knows which field it dispatches on.
    keyed_by: Option<Rc<str>>,
}

impl Claim {
    /// A claim carrying nothing beyond being made — a decorator's, or a handler for a
    /// name with neither keys nor a payload.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same claim, covering `keys` — a producer's claim. Adds to any already
    /// claimed, since one name's producers are merged.
    #[must_use]
    pub fn with_keys<'a>(mut self, keys: impl IntoIterator<Item = &'a str>) -> Self {
        self.keys.extend(keys.into_iter().map(Rc::from));
        self.keyed = true;
        self
    }

    /// The same claim, taking a payload of the declared record `record`, keyed by its
    /// field `keyed_by` where the handlers dispatch on one.
    #[must_use]
    pub fn with_payload(mut self, record: &str, keyed_by: Option<&str>) -> Self {
        self.payload = Some(Rc::from(record));
        self.keyed_by = keyed_by.map(Rc::from);
        self
    }

    /// Every key this claim covers, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().map(|key| &**key)
    }

    /// The payload field whose value is a mark's key, where the claim names one.
    #[must_use]
    pub fn keyed_by(&self) -> Option<&str> {
        self.keyed_by.as_deref()
    }

    /// What this claim says about `key`: `None` where none was asked about, or where the
    /// claim lists none and so covers whatever comes.
    fn covers(&self, key: Option<&str>) -> Option<bool> {
        let key = key.filter(|_| self.keyed)?;
        Some(self.keys.iter().any(|claimed| &**claimed == key))
    }
}

/// What a host's declaration says about one mark name, asked about one key (D42).
///
/// This is the whole of what compiling a `mark()` or a code block consumes of a claim —
/// the warning at a name, the warning at a key, and the record a payload is checked
/// against all read it and nothing else — which is why it is also what the unit records
/// as its fact ([`ClaimRef`](crate::ClaimRef)). The one thing it leaves out is the key
/// list a did-you-mean is drawn from, so that text can go stale where the answer itself
/// did not change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimAnswer {
    /// The host declares no handlers, so it says nothing about any name.
    Undeclared,
    /// The host declares handlers, and none claims this name.
    Unclaimed,
    /// A handler claims it.
    Claimed {
        /// Whether a producer covers the key asked about: `None` where no key was asked
        /// about, or where the name's claims list none and so cover every key.
        key: Option<bool>,
        /// The record a mark of this name carries, where the handlers take a payload.
        payload: Option<Rc<str>>,
        /// The payload field whose value is the mark's key.
        keyed_by: Option<Rc<str>>,
    },
}

impl Host {
    /// A host that names nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A host that names each of `names` without typing it — the shape a host has
    /// before it declares anything, and what the Phase 0 spike passes.
    #[must_use]
    pub fn untyped<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            ambient: names
                .into_iter()
                .map(|name| (Rc::from(name), Ambient::Untyped))
                .collect(),
            ..Self::new()
        }
    }

    /// Adds an ambient name.
    #[must_use]
    pub fn bind(mut self, name: &str, ambient: Ambient) -> Self {
        self.ambient.push((Rc::from(name), ambient));
        self
    }

    /// Names the ambient value a file's record retypes.
    #[must_use]
    pub fn documents(mut self, name: &str) -> Self {
        self.document = Some(Rc::from(name));
        self
    }

    /// Names the field a page is rendered through.
    #[must_use]
    pub fn rendered_through(mut self, field: &str) -> Self {
        self.entry = Some(Rc::from(field));
        self
    }

    /// Declares the assets the host's build produced, as `(logical path, URL)` pairs.
    ///
    /// Replaces any already declared rather than adding to them: a manifest is one
    /// build's whole answer, and merging two would let a stale entry outlive the build
    /// that made it.
    #[must_use]
    pub fn with_assets<'a>(
        mut self,
        assets: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        self.assets = assets
            .into_iter()
            .map(|(path, url)| (Rc::from(path), Rc::from(url)))
            .collect();
        self
    }

    /// The URL the asset at `path` shipped at, if the host's build made one.
    #[must_use]
    pub fn asset(&self, path: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(logical, _)| &**logical == path)
            .map(|(_, url)| &**url)
    }

    /// Every logical path the host declared, in declaration order — what a miss is
    /// offered a did-you-mean from, and listed by.
    pub fn asset_paths(&self) -> impl Iterator<Item = &str> {
        self.assets.iter().map(|(path, _)| &**path)
    }

    /// Declares the grammars code is highlighted with, as `(path, .sublime-syntax text)`
    /// pairs. Where two declare the same scope, language, or name, the later one wins,
    /// so grammars a site adds after a default set take precedence over it.
    ///
    /// Replaces any already declared, as [`with_assets`](Self::with_assets) does. An
    /// empty list still asks for highlighting — and so for a warning at every language
    /// no grammar answers — where a host that never calls this asks for none.
    #[must_use]
    pub fn with_grammars<'a>(
        mut self,
        grammars: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        self.grammars = Some(Grammars::new(grammars));
        self
    }

    /// This declaration, holding `previous`'s grammars where the two declare the same
    /// ones: a resident core handed a new declaration — a rebuild that re-hashed an
    /// asset, say — then keeps the set it already parsed instead of parsing every grammar
    /// again at the next code block. Equal either way, so nothing a comparison of the
    /// two declarations decides changes.
    #[must_use]
    pub fn keeping_grammars_of(mut self, previous: &Self) -> Self {
        if self.grammars.is_some() && self.grammars == previous.grammars {
            self.grammars.clone_from(&previous.grammars);
        }
        self
    }

    /// Declares the mark names the host's handlers claim, each carrying nothing beyond
    /// being made — as the shell's default plugin claims `styles`, `scripts` and `head`
    /// (D42).
    ///
    /// Replaces any already declared, as [`with_assets`](Self::with_assets) does. An
    /// empty list still declares handlers — and so warns at every name none of them
    /// claims — where a host that never calls this warns at none.
    #[must_use]
    pub fn with_marks<'a>(self, names: impl IntoIterator<Item = &'a str>) -> Self {
        self.with_claims(names.into_iter().map(|name| (name, Claim::new())))
    }

    /// Declares what the host's handlers claim, by mark name: the keys a producer
    /// covers and the payload a mark of that name carries (D42).
    ///
    /// Replaces any already declared, like [`with_marks`](Self::with_marks). Two entries
    /// for one name are merged as [`Claim`] describes.
    #[must_use]
    pub fn with_claims<'a>(
        mut self,
        claims: impl IntoIterator<Item = (&'a str, Claim)>,
    ) -> Self {
        let mut declared: BTreeMap<Rc<str>, Claim> = BTreeMap::new();
        for (name, claim) in claims {
            let merged = declared.entry(Rc::from(name)).or_default();
            merged.keys.extend(claim.keys.iter().cloned());
            merged.keyed |= claim.keyed;
            merged.payload = merged.payload.take().or(claim.payload);
            merged.keyed_by = merged.keyed_by.take().or(claim.keyed_by);
        }
        self.marks = Some(declared);
        self
    }

    /// What the host's handlers claim for `name`, where any claims it.
    #[must_use]
    pub fn claim(&self, name: &str) -> Option<&Claim> {
        self.marks.as_ref()?.get(name)
    }

    /// What this declaration says about the mark name `name`, asked about `key`.
    #[must_use]
    pub fn answer(&self, name: &str, key: Option<&str>) -> ClaimAnswer {
        match self.marks.as_ref().map(|marks| marks.get(name)) {
            None => ClaimAnswer::Undeclared,
            Some(None) => ClaimAnswer::Unclaimed,
            Some(Some(claim)) => ClaimAnswer::Claimed {
                key: claim.covers(key),
                payload: claim.payload.clone(),
                keyed_by: claim.keyed_by.clone(),
            },
        }
    }

    /// Every name the host's handlers claim, in sorted order — the candidates a
    /// did-you-mean is drawn from.
    pub fn claimed_names(&self) -> impl Iterator<Item = &str> {
        self.marks.iter().flatten().map(|(name, _)| &**name)
    }

    /// The grammars the host declared, if it asked for highlighting.
    pub(crate) const fn grammars(&self) -> Option<&Grammars> {
        self.grammars.as_ref()
    }

    /// What a document highlighted against this declaration records: a digest of the
    /// grammars, or `None` where the host declared none.
    pub(crate) fn grammar_digest(&self) -> Option<u64> {
        self.grammars.as_ref().map(Grammars::digest)
    }

    /// How `name` is bound where this is read, if the host names it at all: the
    /// declaration's binding, or — for the document value in a file's view — what that
    /// file's record made it.
    #[must_use]
    pub fn binding(&self, name: &str) -> Option<&Ambient> {
        let declared = self
            .ambient
            .iter()
            .find(|(bound, _)| &**bound == name)
            .map(|(_, ambient)| ambient)?;
        match &self.document_binding {
            Some(binding) if self.document.as_deref() == Some(name) => Some(binding),
            _ => Some(declared),
        }
    }

    /// The ambient value a file's record retypes, if the host renders documents.
    #[must_use]
    pub fn document(&self) -> Option<&str> {
        self.document.as_deref()
    }

    /// The field a page is rendered through, if the host renders pages.
    #[must_use]
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref()
    }

    /// Every name the host binds, in declaration order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.ambient.iter().map(|(name, _)| &**name)
    }

    /// Whether the top-level bindings of the file at `path` are an ambient value's
    /// fields — the file whose bindings need declaring, and the one whose record is
    /// synthesized as it is prepared.
    #[must_use]
    pub fn binds_from(&self, path: &str) -> bool {
        self.ambient.iter().any(
            |(_, ambient)| matches!(ambient, Ambient::Bindings(file) if &**file == path),
        )
    }

    /// The ambient name whose fields are the top-level bindings of the file at `path`,
    /// if the host reads one from it.
    #[must_use]
    pub fn bindings_value(&self, path: &str) -> Option<&str> {
        self.ambient
            .iter()
            .find_map(|(name, ambient)| match ambient {
                Ambient::Bindings(file) if &**file == path => Some(&**name),
                _ => None,
            })
    }

    /// Every ambient value the host reads from a file's bindings, as `(value, file)`,
    /// in declaration order — the files a `from` binding may stand in.
    pub fn bindings_files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.ambient
            .iter()
            .filter_map(|(name, ambient)| match ambient {
                Ambient::Bindings(file) => Some((&**name, &**file)),
                _ => None,
            })
    }

    /// The host as the file at `path` sees it: a file the host gave a `record` reads
    /// the document value as that record, and a document the host gave none reads it
    /// unknown-typed — D30's document that no glob matched, which renders all the same.
    ///
    /// The declaration itself is never rewritten. The type a page schema extends and
    /// the field a page renders through stay what the host declared, whatever one file
    /// was compiled as.
    #[must_use]
    pub fn for_file(&self, path: &str, record: Option<&str>) -> Self {
        let mut seen = self.clone();
        let Some(document) = seen.document.clone() else {
            return seen;
        };
        if !seen.ambient.iter().any(|(name, _)| *name == document) {
            return seen;
        }
        seen.document_binding = match record {
            Some(record) => Some(Ambient::Named(Rc::from(record))),
            None if crate::is_document(path) => Some(Ambient::Untyped),
            None => return seen,
        };
        seen
    }

    /// The type every page schema extends: what the host *declared* its document value
    /// to be, never what one file's record made it. `None` unless the host both names a
    /// document value and types it.
    #[must_use]
    pub fn document_type(&self) -> Option<Rc<str>> {
        let document = self.document.as_deref()?;
        let declared = self
            .ambient
            .iter()
            .find(|(name, _)| &**name == document)
            .map(|(_, ambient)| ambient)?;
        match declared {
            Ambient::Named(name) => Some(name.clone()),
            Ambient::Untyped | Ambient::Bindings(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::GRAMMARS;

    fn shell() -> Host {
        Host::new()
            .bind("page", Ambient::named("Page"))
            .bind("build", Ambient::named("Build"))
            .bind("site", Ambient::bindings("site.lem"))
            .documents("page")
            .rendered_through("layout")
    }

    fn built() -> Host {
        Host::new().with_assets([
            ("styles/site.css", "/site.abc123.css"),
            ("scripts/gallery.js", "/gallery.def456.js"),
        ])
    }

    #[test]
    fn reads_an_asset_url_out_of_the_manifest_and_nothing_out_of_a_path_it_has_not_got() {
        assert_eq!(built().asset("styles/site.css"), Some("/site.abc123.css"));
        assert_eq!(built().asset("styles/other.css"), None);
        assert_eq!(Host::new().asset("styles/site.css"), None);
    }

    #[test]
    fn lists_the_asset_paths_in_the_order_the_host_declared_them() {
        assert_eq!(
            built().asset_paths().collect::<Vec<_>>(),
            ["styles/site.css", "scripts/gallery.js"]
        );
        assert_eq!(Host::new().asset_paths().count(), 0);
    }

    #[test]
    fn replaces_the_manifest_rather_than_adding_to_it() {
        // A manifest is one build's whole answer. Merging would let an asset the last
        // build dropped go on resolving, which is the stale URL this exists to catch.
        let rebuilt = built().with_assets([("styles/site.css", "/site.999.css")]);
        assert_eq!(rebuilt.asset("styles/site.css"), Some("/site.999.css"));
        assert_eq!(rebuilt.asset("scripts/gallery.js"), None);
    }

    #[test]
    fn keeps_the_grammars_it_parsed_for_a_declaration_that_repeats_them() {
        let before = Host::new().with_grammars(GRAMMARS.iter().copied());
        before.grammars().unwrap().lookup("sh");
        let rebuilt = built()
            .with_grammars(GRAMMARS.iter().copied())
            .keeping_grammars_of(&before);
        assert!(rebuilt.grammars().unwrap().is_parsed());
        // Other grammars are parsed afresh, and a host that declares none keeps none.
        let other = Host::new()
            .with_grammars([GRAMMARS[0]])
            .keeping_grammars_of(&before);
        assert!(!other.grammars().unwrap().is_parsed());
        assert!(
            Host::new()
                .keeping_grammars_of(&before)
                .grammars()
                .is_none()
        );
    }

    #[test]
    fn makes_a_manifest_change_a_change_to_the_declaration() {
        // What the resident `Core` compares to decide whether to recompile (D38).
        assert_ne!(
            built(),
            built().with_assets([("styles/site.css", "/x.css")])
        );
        assert_eq!(built(), built());
    }

    #[test]
    fn says_why_a_path_is_no_asset_and_offers_the_nearest_when_there_is_one() {
        assert_eq!(
            no_asset(&built(), "styles/sight.css"),
            "the manifest has no asset `styles/sight.css`; did you mean \
             `styles/site.css`?"
        );
        assert_eq!(
            no_asset(&built(), "vendor/analytics.js"),
            "the manifest has no asset `vendor/analytics.js`"
        );
        assert_eq!(
            no_asset(&Host::new(), "styles/site.css"),
            "the manifest has no asset `styles/site.css`; the host declares none"
        );
    }

    #[test]
    fn merges_every_claim_made_for_one_name() {
        // What a site's plugin list looks like: a highlighter producing two languages
        // and a copy-button decorator producing none, both answering for `code`. Keys
        // are unioned across them, and the payload the producer named is the name's.
        let host = Host::new().with_claims([
            (
                "code",
                Claim::new()
                    .with_keys(["rust"])
                    .with_payload("Code", Some("language")),
            ),
            ("code", Claim::new()),
            ("code", Claim::new().with_keys(["ts"])),
        ]);
        let claim = host.claim("code").expect("a claim");
        assert_eq!(claim.keys().collect::<Vec<_>>(), ["rust", "ts"]);
        assert_eq!(claim.keyed_by(), Some("language"));
        assert_eq!(
            host.answer("code", Some("ts")),
            ClaimAnswer::Claimed {
                key: Some(true),
                payload: Some(Rc::from("Code")),
                keyed_by: Some(Rc::from("language")),
            }
        );
        // And a key no producer listed stays uncovered, however many handlers claim the
        // name: a decorator transforms what is there, it does not make content.
        assert!(matches!(
            host.answer("code", Some("css")),
            ClaimAnswer::Claimed {
                key: Some(false),
                ..
            }
        ));
    }

    #[test]
    fn covers_every_key_where_no_claim_lists_one() {
        // A bundler claims `styles` and dispatches on nothing, so a key never makes one
        // of its marks unhandled — which is the other half of the rule above.
        let host = Host::new().with_marks(["styles"]);
        assert!(matches!(
            host.answer("styles", Some("anything")),
            ClaimAnswer::Claimed {
                key: None,
                payload: None,
                keyed_by: None
            }
        ));
        assert_eq!(host.answer("head", None), ClaimAnswer::Unclaimed);
        assert_eq!(Host::new().answer("styles", None), ClaimAnswer::Undeclared);
    }

    #[test]
    fn replaces_the_claims_rather_than_adding_to_them() {
        // One plugin list is one whole answer, as a manifest is: a name the last list
        // dropped must stop being claimed, or a stale handler would go on answering.
        let host = Host::new()
            .with_marks(["styles", "head"])
            .with_marks(["styles"]);
        assert_eq!(host.claimed_names().collect::<Vec<_>>(), ["styles"]);
        assert_eq!(host.answer("head", None), ClaimAnswer::Unclaimed);
    }

    #[test]
    fn names_nothing_by_default() {
        assert_eq!(Host::new().names().count(), 0);
        assert_eq!(Host::new().binding("page"), None);
    }

    #[test]
    fn gives_an_untyped_value_the_unknown_type() {
        let host = Host::untyped(["page", "site", "build"]);
        assert_eq!(host.binding("page"), Some(&Ambient::Untyped));
        assert_eq!(host.binding("page").map(Ambient::ty), Some(Type::Any));
    }

    #[test]
    fn types_a_declared_value_by_the_name_the_host_gave() {
        assert_eq!(
            shell().binding("page").map(Ambient::ty),
            Some(Type::Named(Rc::from("Page")))
        );
    }

    #[test]
    fn types_a_bindings_value_by_the_path_it_reads() {
        assert_eq!(
            shell().binding("site").map(Ambient::ty),
            Some(Type::Named(Rc::from("site.lem")))
        );
    }

    #[test]
    fn keeps_the_ambient_names_in_the_order_the_host_gave_them() {
        assert_eq!(
            shell().names().collect::<Vec<_>>(),
            vec!["page", "build", "site"]
        );
    }

    #[test]
    fn knows_which_file_a_value_takes_its_bindings_from() {
        assert!(shell().binds_from("site.lem"));
        assert!(!shell().binds_from("layout.lem"));
        assert_eq!(shell().bindings_value("site.lem"), Some("site"));
        assert_eq!(shell().bindings_value("layout.lem"), None);
        assert_eq!(
            shell().bindings_files().collect::<Vec<_>>(),
            [("site", "site.lem")]
        );
        assert_eq!(Host::untyped(["site"]).bindings_files().count(), 0);
    }

    #[test]
    fn takes_the_page_schemas_base_from_the_document_value() {
        assert_eq!(shell().document_type().as_deref(), Some("Page"));
    }

    #[test]
    fn reads_a_files_own_record_as_the_document_value() {
        let seen = shell().for_file("feed/a/index.md", Some("Article"));
        assert_eq!(
            seen.binding("page"),
            Some(&Ambient::Named(Rc::from("Article")))
        );
    }

    #[test]
    fn leaves_a_document_no_record_matched_unknown_typed() {
        let seen = shell().for_file("feed/a/index.md", None);
        assert_eq!(seen.binding("page"), Some(&Ambient::Untyped));
    }

    #[test]
    fn leaves_a_templates_document_value_as_the_host_declared_it() {
        let seen = shell().for_file("layout.lem", None);
        assert_eq!(
            seen.binding("page"),
            Some(&Ambient::Named(Rc::from("Page")))
        );
    }

    #[test]
    fn keeps_the_page_schema_base_when_a_file_is_compiled_as_a_record() {
        let seen = shell().for_file("feed/a/index.md", Some("Article"));
        assert_eq!(seen.document_type().as_deref(), Some("Page"));
    }

    #[test]
    fn has_nothing_to_retype_for_a_host_that_renders_no_documents() {
        let host = Host::untyped(["page"]);
        assert_eq!(host.for_file("a.md", Some("Article")), host);
    }

    #[test]
    fn retypes_nothing_for_a_document_value_the_host_never_bound() {
        let host = Host::untyped(["site"]).documents("page");
        let seen = host.for_file("a.md", Some("Article"));
        assert_eq!(seen.binding("page"), None);
    }

    #[test]
    fn has_no_page_schema_base_without_a_document_value() {
        assert_eq!(Host::untyped(["page"]).document_type(), None);
    }

    #[test]
    fn has_no_page_schema_base_when_the_document_value_is_untyped() {
        let host = Host::untyped(["page"]).documents("page");
        assert_eq!(host.document_type(), None);
    }

    #[test]
    fn has_no_page_schema_base_when_the_document_value_is_not_bound() {
        assert_eq!(shell().documents("nothing").document_type(), None);
    }
}
