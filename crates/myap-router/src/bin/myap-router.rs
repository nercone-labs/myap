//! The command-line tool of Myap Router.
//!
//! ```text
//! myap-router build <input.osm.pbf | overpass.json> <output.myapgraph>
//! myap-router route <graph.myapgraph> <lat,lon> <lat,lon>... [--mode car|bicycle|foot] [--alternatives] [--avoid tolls,highways,ferries,unpaved,narrow,steps,hills,level_crossings,crossing_turns,unlit]
//!     [--optimize fastest|shortest|eco|simple] [--left]
//! myap-router query <south,west,north,east>
//! ```

use geo::{Bounds, Coordinate};
use lexopt::prelude::*;
use myap_router::{Graph, GraphBuilder, Router};
use routing::{DrivingSide, Optimize, RouteRequest, TravelMode};
use std::fs::File;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

fn coordinate(text: &str) -> Result<Coordinate, String> {
    let (a, b) = text.split_once(',').ok_or_else(|| format!("not lat,lon: {text}"))?;
    let (a, b) = (a.trim().parse().map_err(|_| format!("not a number: {a}"))?, b.trim().parse().map_err(|_| format!("not a number: {b}"))?);
    Coordinate::checked(a, b).ok_or_else(|| format!("out of range: {text}"))
}

fn run() -> Result<(), String> {
    let mut parser = lexopt::Parser::from_env();
    let mut positional: Vec<String> = Vec::new();
    let mut mode = TravelMode::Car;
    let mut alternatives = false;
    let mut avoid = routing::Avoid::default();
    let mut optimize = Optimize::Fastest;
    let mut side = DrivingSide::Right;
    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        match arg {
            Long("mode") => {
                mode = match parser.value().map_err(|e| e.to_string())?.string().map_err(|_| "invalid mode")?.as_str() {
                    "car" => TravelMode::Car,
                    "bicycle" => TravelMode::Bicycle,
                    "foot" => TravelMode::Foot,
                    other => return Err(format!("unknown mode {other}")),
                }
            }
            Long("alternatives") => alternatives = true,
            Long("left") => side = DrivingSide::Left,
            Long("optimize") => {
                optimize = match parser.value().map_err(|e| e.to_string())?.string().map_err(|_| "invalid optimize")?.as_str() {
                    "fastest" => Optimize::Fastest,
                    "shortest" => Optimize::Shortest,
                    "eco" => Optimize::Eco,
                    "simple" => Optimize::Simple,
                    other => return Err(format!("unknown optimisation {other}")),
                }
            }
            Long("avoid") => {
                for item in parser.value().map_err(|e| e.to_string())?.string().map_err(|_| "invalid avoid")?.split(',') {
                    match item {
                        "tolls" => avoid.tolls = true,
                        "highways" => avoid.highways = true,
                        "ferries" => avoid.ferries = true,
                        "unpaved" => avoid.unpaved = true,
                        "narrow" => avoid.narrow = true,
                        "steps" => avoid.steps = true,
                        "hills" => avoid.hills = true,
                        "level_crossings" => avoid.level_crossings = true,
                        "crossing_turns" => avoid.crossing_turns = true,
                        "unlit" => avoid.unlit = true,
                        other => return Err(format!("unknown avoidance {other}")),
                    }
                }
            }
            Value(v) => positional.push(v.string().map_err(|_| "invalid argument")?),
            other => return Err(other.unexpected().to_string()),
        }
    }
    match positional.first().map(String::as_str) {
        Some("build") if positional.len() == 3 => {
            let (input, output) = (PathBuf::from(&positional[1]), PathBuf::from(&positional[2]));
            let started = Instant::now();
            let (graph, stats) = if input.extension().is_some_and(|e| e == "json") {
                let elements = osm::Overpass::parse(File::open(&input).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                GraphBuilder::from_elements(&elements).map_err(|e| e.to_string())?
            } else {
                GraphBuilder::from_pbf(&input, |p| eprint!("\r{:.0} %", p * 100.0)).map_err(|e| e.to_string())?
            };
            graph.write(&mut File::create(&output).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            eprintln!("\n{stats:?} in {:.1} s", started.elapsed().as_secs_f64());
            Ok(())
        }
        Some("route") if positional.len() >= 4 => {
            let started = Instant::now();
            let graph = Graph::read(&mut File::open(&positional[1]).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let mut router = Router::new(graph);
            eprintln!("loaded in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
            let waypoints = positional[2..].iter().map(|t| coordinate(t)).collect::<Result<Vec<_>, _>>()?;
            let mut request = RouteRequest::new(waypoints, mode);
            request.alternatives = alternatives;
            request.avoid = avoid;
            request.optimize = optimize;
            request.driving_side = side;
            let started = Instant::now();
            let routes = router.route(&request).map_err(|e| e.to_string())?;
            eprintln!("routed in {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
            for (i, route) in routes.iter().enumerate() {
                println!("route {i}: {:.0} m, {:.0} s, via {}", route.distance_m, route.duration_s, route.summary);
                if let Some((up, down)) = route.climb_m(2.0) {
                    println!("  climb {up:.0} m, descent {down:.0} m");
                }
                for alert in &route.alerts {
                    println!("  alert {:?} at point {}", alert.kind, alert.index);
                }
                for step in route.steps() {
                    let m = &step.maneuver;
                    println!(
                        "  {:?} {:?} exit={:?} {} ({:.0} m){}{}",
                        m.kind,
                        m.modifier,
                        m.exit,
                        step.name.as_deref().or(step.road_ref.as_deref()).unwrap_or("-"),
                        step.distance_m,
                        step.junction.as_deref().map(|j| format!(" at {j}")).unwrap_or_default(),
                        if step.lanes.is_empty() {
                            String::new()
                        } else {
                            format!(" lanes {}", step.lanes.iter().map(|l| if l.valid { '↑' } else { '·' }).collect::<String>())
                        }
                    );
                }
            }
            Ok(())
        }
        Some("query") if positional.len() == 2 => {
            let v: Vec<f64> = positional[1].split(',').map(|x| x.trim().parse::<f64>().map_err(|_| format!("not a number: {x}"))).collect::<Result<_, _>>()?;
            let [south, west, north, east] = v[..] else { return Err("bounds are south,west,north,east".into()) };
            println!("{}", GraphBuilder::overpass_query(&Bounds { south, west, north, east }));
            Ok(())
        }
        _ => Err("usage: myap-router build <input> <output> | route <graph> <lat,lon> <lat,lon>... | query <s,w,n,e>".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("myap-router: {e}");
            ExitCode::FAILURE
        }
    }
}
