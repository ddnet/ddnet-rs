use std::{sync::atomic::AtomicU64, time::Duration};

use owo_colors::{DynColors, OwoColorize};
use tracing::{Level, Span};
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::Context, prelude::*, registry::LookupSpan,
};

const PALETTE: [DynColors; 8] = [
    DynColors::Ansi(owo_colors::AnsiColors::Green),
    DynColors::Ansi(owo_colors::AnsiColors::Blue),
    DynColors::Ansi(owo_colors::AnsiColors::Magenta),
    DynColors::Ansi(owo_colors::AnsiColors::Cyan),
    DynColors::Ansi(owo_colors::AnsiColors::Yellow),
    DynColors::Ansi(owo_colors::AnsiColors::Red),
    DynColors::Ansi(owo_colors::AnsiColors::BrightBlue),
    DynColors::Ansi(owo_colors::AnsiColors::BrightMagenta),
];

#[cfg(not(target_arch = "wasm32"))]
type Instant = std::time::Instant;

#[cfg(target_arch = "wasm32")]
// Minimal Instant wrapper used here to avoid platform-specific hacks.
#[derive(Clone, Copy)]
pub struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        Self
    }
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

#[derive(Default)]
struct SpanFields {
    // Optional override label for span name (used by Benchmark::bench[_multi]).
    label: Option<String>,
    // If provided, use this duration (in milliseconds) instead of measured time.
    bench_ns: Option<u64>,
    // If provided, mark this node to be force-rendered even if tree is skipped.
    bench_force: bool,
    // If provided on the root span, skip rendering the whole tree.
    bench_skip_render: bool,
}

struct FieldVisitor<'a> {
    // Where to write parsed fields
    out: &'a mut SpanFields,
}

impl<'a> tracing::field::Visit for FieldVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        // Accept debug string for label or bench_ns if user passes non-typed values.
        let name = field.name();
        if name == "bench_label" {
            self.out.label = Some(format!("{:?}", value));
        } else if name == "bench_ns" {
            // Try to parse from Debug string into f64; fallback silently.
            let s = format!("{:?}", value);
            if let Ok(v) = s.parse::<u64>() {
                self.out.bench_ns = Some(v);
            }
        } else if name == "bench_force" {
            let s = format!("{:?}", value);
            match s.as_str() {
                "true" => self.out.bench_force = true,
                "false" => self.out.bench_force = false,
                _ => {}
            }
        } else if name == "bench_skip_render" {
            let s = format!("{:?}", value);
            match s.as_str() {
                "true" => self.out.bench_skip_render = true,
                "false" => self.out.bench_skip_render = false,
                _ => {}
            }
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "bench_label" {
            self.out.label = Some(value.to_string());
        }
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "bench_ns" {
            self.out.bench_ns = Some(value);
        }
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        if field.name() == "bench_force" {
            self.out.bench_force = value;
        } else if field.name() == "bench_skip_render" {
            self.out.bench_skip_render = value;
        }
    }
}

struct SpanData {
    start: Instant,
    depth: usize,
    label: Option<String>,
    bench_ns: Option<u64>,
    bench_force: bool,
    bench_skip_render: bool,
    children: Vec<Node>,
}

struct Node {
    name: String,
    ns: u64,
    depth: usize,
    force_render: bool,
    skip_render: bool,
    children: Vec<Node>,
}

struct BenchLayer {
    benchmark_ignore_under: Duration,
}

impl<S> Layer<S> for BenchLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        // Collect custom fields (bench_label, bench_ns)
        let mut sf = SpanFields::default();
        let mut visitor = FieldVisitor { out: &mut sf };
        attrs.record(&mut visitor);

        // Determine depth from parent span if any
        let depth = ctx
            .span(id)
            .and_then(|span| span.parent())
            .and_then(|p| p.extensions().get::<SpanData>().map(|d| d.depth + 1))
            .unwrap_or(0);

        if let Some(span) = ctx.span(id) {
            let mut ex = span.extensions_mut();
            ex.insert(SpanData {
                start: Instant::now(),
                depth,
                label: sf.label,
                bench_ns: sf.bench_ns,
                bench_force: sf.bench_force,
                bench_skip_render: sf.bench_skip_render,
                children: Vec::new(),
            });
        }
    }

    fn on_close(&self, id: tracing::span::Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            let meta = span.metadata();
            let mut ex = span.extensions_mut();
            if let Some(data) = ex.remove::<SpanData>() {
                // Determine duration: prefer explicit bench_ns, else measured elapsed
                let ns = data
                    .bench_ns
                    .unwrap_or_else(|| data.start.elapsed().as_nanos() as u64);

                // Resolve label: explicit label or enriched span name including crate.
                // For spans created via #[instrument], `meta.name()` is the function name.
                // Prepend the crate (first segment of module_path) when no explicit label is set.
                let name = data.label.unwrap_or_else(|| {
                    meta.module_path().map_or_else(
                        || format!("{}::{}", meta.target(), meta.name()),
                        |mp| format!("{}::{}", mp, meta.name()),
                    )
                });

                // Build a node that includes any aggregated children
                let node = Node {
                    name,
                    ns,
                    depth: data.depth,
                    force_render: data.bench_force,
                    skip_render: data.bench_skip_render,
                    children: data.children,
                };

                // If parent exists, attach to it; otherwise, render the full tree now.
                if let Some(parent) = span.parent() {
                    if let Some(parent_data) = parent.extensions_mut().get_mut::<SpanData>() {
                        parent_data.children.push(node);
                    }
                } else {
                    // Render the tree based on threshold policy.
                    print_tree_root_policy(&node, &self.benchmark_ignore_under);
                }
            }
        }
    }
}

fn print_tree(node: &Node, benchmark_ignore_under: &Duration) {
    if !node.skip_render {
        // Print current node
        let indent = "  ".repeat(node.depth);
        let left = format!("{}{}", indent, node.name);
        let ms = node.ns as f64 / 1_000_000.0;
        let time_str = format!("{:>10.4} ms", ms);
        // Add a unicode lag icon if this node exceeds the skip threshold
        let lag_icon = if node.ns >= benchmark_ignore_under.as_nanos() as u64 {
            // turtle
            "\u{1F422} "
        } else {
            "   "
        };
        let color = PALETTE[node.depth % PALETTE.len()];
        println!("{}{} {}", lag_icon, time_str.color(color), left);
    }

    // Print children in insertion order
    for child in &node.children {
        print_tree(child, benchmark_ignore_under);
    }
}

fn print_tree_root_policy(root: &Node, benchmark_ignore_under: &Duration) {
    if root.ns >= benchmark_ignore_under.as_nanos() as u64 {
        // Print the full tree normally.
        print_tree(root, benchmark_ignore_under);
    } else {
        // Tree is below threshold; only print forced nodes (and their subtrees as-is).
        print_tree_forced_only(root, benchmark_ignore_under);
    }
}

fn print_tree_forced_only(node: &Node, benchmark_ignore_under: &Duration) {
    if node.force_render {
        // Print this node and its entire subtree normally.
        print_tree(node, benchmark_ignore_under);
    } else {
        for child in &node.children {
            print_tree_forced_only(child, benchmark_ignore_under);
        }
    }
}

/// Initialize the global tracing subscriber with the custom benchmark layer and filter level.
/// Safe to call multiple times; subsequent calls are ignored.
fn global_init_with_level(benchmark_ignore_under: Option<Duration>, level: Level) {
    // Skip rendering an entire tree if it took less than this threshold.
    const TREE_SKIP_THRESHOLD: Duration = Duration::from_nanos(17_000_000);
    let filter = EnvFilter::from_default_env()
        .add_directive(level.into())
        .add_directive("winit=off".parse().unwrap())
        .add_directive("quinn=off".parse().unwrap());
    let subscriber = Registry::default().with(filter).with(BenchLayer {
        benchmark_ignore_under: benchmark_ignore_under.unwrap_or(TREE_SKIP_THRESHOLD),
    });
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Initialize with TRACE level and default env filter.
pub fn global_init(benchmark_ignore_under: Option<Duration>) {
    global_init_with_level(benchmark_ignore_under, Level::TRACE);
}

pub struct Benchmark {
    start_time: Option<Instant>,
    cur_diff: AtomicU64,

    // Root span used for grouping; keep as Span so Benchmark is Send.
    root_span: Option<Span>,
}

impl Benchmark {
    pub fn new(do_bench: bool) -> Self {
        let start_time = if do_bench { Some(Instant::now()) } else { None };

        // Create a root span to collect all subsequent bench nodes for indentation.
        let root_span = if do_bench {
            Some(tracing::span!(
                Level::INFO,
                "benchmark",
                bench_label = %"benchmark",
                bench_skip_render = true
            ))
        } else {
            None
        };

        Self {
            start_time,
            cur_diff: AtomicU64::new(0),
            root_span,
        }
    }

    /// does not overwrite current time
    pub fn bench_multi(&self, name: &str) -> u64 {
        if self.root_span.is_none() {
            return 0;
        }
        let cur_diff = self.cur_diff.load(std::sync::atomic::Ordering::SeqCst);
        let diff_ns = self.start_time.unwrap().elapsed().as_nanos() as u64 - cur_diff;

        // Enter root only for this call so the child span nests implicitly.
        // The guard does not cross an await, keeping this Send-friendly.
        if let Some(root) = &self.root_span {
            let _ = tracing::span!(
                parent: root,
                Level::INFO,
                "bench",
                bench_label = %name,
                bench_ns = diff_ns,
                bench_force = true
            );
        }

        diff_ns + cur_diff
    }

    pub fn bench(&self, name: &str) {
        if self.root_span.is_some() {
            self.cur_diff
                .store(self.bench_multi(name), std::sync::atomic::Ordering::SeqCst);
        }
    }
}
