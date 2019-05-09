extern crate clap;
extern crate noria;
extern crate rand;
extern crate slog;

mod test_populate;

use noria::{Builder, DataType, LocalAuthority, ReuseConfigType, SyncHandle};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::{thread, time};

pub struct Backend {
    g: SyncHandle<LocalAuthority>,
}

impl Backend {
    pub fn new(partial: bool, _shard: bool, reuse: &str) -> Backend {
        let mut cb = Builder::default();
        let log = noria::logger_pls();
        let blender_log = log.clone();

        if !partial {
            cb.disable_partial();
        }

        cb.log_with(blender_log);

        match reuse {
            "finkelstein" => cb.set_reuse(ReuseConfigType::Finkelstein),
            "full" => cb.set_reuse(ReuseConfigType::Full),
            "noreuse" => cb.set_reuse(ReuseConfigType::NoReuse),
            "relaxed" => cb.set_reuse(ReuseConfigType::Relaxed),
            _ => panic!("reuse configuration not supported"),
        }

        let g = cb.start_simple().unwrap();

        Backend { g }
    }

    fn login(&mut self, user_context: HashMap<String, DataType>) -> Result<(), String> {
        self.g
            .on_worker(|w| w.create_universe(user_context.clone()))
            .unwrap();

        Ok(())
    }

    fn set_security_config(&mut self, config_file: &str) {
        let mut config = String::new();
        let mut cf = File::open(config_file).unwrap();
        cf.read_to_string(&mut config).unwrap();

        // Install recipe with policies
        self.g.on_worker(|w| w.set_security_config(config)).unwrap();
    }

    fn migrate(
        &mut self,
        schema_file: &str,
        query_file: Option<&str>,
        ext: bool,
    ) -> Result<(), String> {
        // Read schema file
        let mut sf = File::open(schema_file).unwrap();
        let mut s = String::new();
        sf.read_to_string(&mut s).unwrap();

        let mut rs = s.clone();
        s.clear();

        // Read query file
        match query_file {
            None => (),
            Some(qf) => {
                let mut qf = File::open(qf).unwrap();
                qf.read_to_string(&mut s).unwrap();
                rs.push_str("\n");
                rs.push_str(&s);
            }
        }

        // Install recipe
        if ext {
            self.g.extend_recipe(&rs).unwrap();
        } else {
            self.g.install_recipe(&rs).unwrap();
        }

        Ok(())
    }
}

fn make_user(name: &str) -> HashMap<String, DataType> {
    let mut user = HashMap::new();
    user.insert(String::from("id"), name.into());

    user
}

fn main() {
    use clap::{App, Arg};
    let args = App::new("SecureCRP")
        .version("0.1")
        .about("Benchmarks HotCRP-like application with security policies.")
        .arg(
            Arg::with_name("schema")
                .short("s")
                .required(true)
                .default_value("noria-benchmarks/securecrp/jeeves_schema.sql")
                .help("SQL schema file"),
        )
        .arg(
            Arg::with_name("queries")
                .short("q")
                .required(true)
                .default_value("noria-benchmarks/securecrp/scraped_queries.sql")
                .help("SQL query file"),
        )
        .arg(
            Arg::with_name("policies")
                .long("policies")
                .required(true)
                .default_value("noria-benchmarks/securecrp/jeeves_policies.json")
                .help("Security policies file"),
        )
        .arg(
            Arg::with_name("graph")
                .short("g")
                .default_value("graph.gv")
                .help("File to dump graph"),
        )
        .arg(
            Arg::with_name("reuse")
                .long("reuse")
                .default_value("full")
                .possible_values(&["noreuse", "finkelstein", "relaxed", "full"])
                .help("Query reuse algorithm"),
        )
        .arg(
            Arg::with_name("shard")
                .long("shard")
                .help("Enable sharding"),
        )
        .arg(
            Arg::with_name("partial")
                .long("partial")
                .help("Enable partial materialization"),
        )
        .arg(
            Arg::with_name("populate")
                .long("populate")
                .default_value("before")
                .help("Populate app with randomly generated data"),
        )
        .arg(Arg::with_name("user").long("user").default_value("1"))
        .get_matches();

    println!("Starting SecureCRP...");

    // Read arguments
    let sloc = args.value_of("schema").unwrap();
    let qloc = args.value_of("queries").unwrap();
    let ploc = args.value_of("policies").unwrap();
    let gloc = args.value_of("graph");
    let partial = args.is_present("partial");
    let shard = args.is_present("shard");
    let reuse = args.value_of("reuse").unwrap();
    let user = args.value_of("user").unwrap();

    println!("user: {}", user);

    let mut backend = Backend::new(partial, shard, reuse);
    backend.migrate(sloc, None, false).unwrap();
    println!("first mig");
    backend.set_security_config(ploc);
    println!("set sec config");
    backend.migrate(sloc, Some(qloc), false).unwrap();
    println!("second mig");
    thread::sleep(time::Duration::from_millis(2000));

    if args.is_present("populate") {
        println!("populating");
        //        test_populate::create_single_trigger_data(&mut backend);
        test_populate::create_users(&mut backend);
        test_populate::create_papers(&mut backend);
    }

    thread::sleep(time::Duration::from_millis(2000));
    backend.login(make_user(user)).is_ok();
    thread::sleep(time::Duration::from_millis(2000));
    println!("user logged in");

    //    test_populate::dump_all_papers(&mut backend);

    // Must happen after user login, since GroupContexts are not created until then.
    //    backend
    //        .migrate(sloc, Some("jeeves_gpcontext_queries.sql"), true)
    //        .unwrap();
    //    println!("third mig");
    /*
    // Check author membership view
    let mut getter = backend.g.view("authors").unwrap();
    let mut query_results = Vec::new();
    query_results.push(getter.lookup(&["2".into()], true).unwrap());
    query_results.push(getter.lookup(&["lara".into()], true).unwrap());
    query_results.push(getter.lookup(&["malte".into()], true).unwrap());
    println!("author membership view: {:?}", query_results);

        // Check reviewer membership view
            let mut getter = backend.g.view("reviewers").unwrap();
            let mut query_results = Vec::new();
            query_results.push(getter.lookup(&["4".into()], true).unwrap());
            query_results.push(getter.lookup(&["lara".into()], true).unwrap());
            query_results.push(getter.lookup(&["malte".into()], true).unwrap());
            query_results.push(getter.lookup(&[0.into()], true).unwrap());
            println!("reviewer membership view: {:?}", query_results);

            // Check chair membership view
            let mut getter = backend.g.view("chairs").unwrap();
            let mut query_results = Vec::new();
            query_results.push(getter.lookup(&["3".into()], true).unwrap());
            query_results.push(getter.lookup(&["kohler".into()], true).unwrap());
            query_results.push(getter.lookup(&[0.into()], true).unwrap());
            println!("chair membership view: {:?}", query_results);
    */

    if gloc.is_some() {
        let graph_fname = gloc.unwrap();
        let mut gf = File::create(graph_fname).unwrap();
        assert!(write!(gf, "{}", backend.g.graphviz().unwrap()).is_ok());
    }

    /*
        // Check AuthorContext
        let mut getter = backend.g.view("AuthorContext").unwrap();
        let mut query_results = Vec::new();
        // Look up by id (corresponds to Paper.id)
        for i in 0..6 {
            query_results.push(getter.lookup(&[i.into()], true).unwrap());
        }
        println!("GroupContext_authors_1: {:?}", query_results);
    */

    thread::sleep(time::Duration::from_millis(2000));
    println!("sleep1");
    thread::sleep(time::Duration::from_millis(2000));
    println!("sleep2");
    /*
        let mut getter = backend.g.view("Authors5").unwrap();
        let mut query_results = Vec::new();
        query_results.push(getter.lookup(&[0.into()], true).unwrap());
        println!(
            "GroupContext_authors_5, bogokey lookup: {:?}",
            query_results
        );

        let mut getter = backend.g.view("AuthorPaperList").unwrap();
        let mut query_results = Vec::new();
        for i in 0..6 {
            query_results.push(getter.lookup(&[i.into()], true).unwrap());
        }
        //query_results.push(getter.lookup(&[0.into()], true).unwrap()); // empty (bogokey does exist)
        println!(
            "authors5 PaperList, bogokey & by-key lookup: {:?}",
            query_results
        );

        let mut getter = backend.g.view("ChairPaperList").unwrap();
        let mut query_results = Vec::new();
        query_results.push(getter.lookup(&[0.into()], true).unwrap()); // empty (bogokey does exist)
        println!("chairs PaperList, bogokey lookup: {:?}", query_results);

    println!("Reading from LatestPaperVersion");
    let mut getter = backend.g.view("LatestPaperVersion").unwrap().into_sync();
    let mut query_results = Vec::new();
    for i in 0..6 {
        query_results.push(getter.lookup(&[i.into()], true).unwrap());
    }
    println!("LatestPaperVersion: {:?}", query_results);
     */

    println!("{}", backend.g.graphviz().unwrap());
    test_populate::dump_query(&mut backend, "ReviewList", 0); // bogokey lookup
    test_populate::dump_query(&mut backend, "PapersReviewed", 0); // by-key lookup
    test_populate::dump_query(&mut backend, "PapersReviewedPrelim", 0); // by-key lookup
//    test_populate::dump_query(&mut backend, "PapersReviewed", 0); // bogokey lookup
//    test_populate::dump_query(&mut backend, "PapersReviewed", 5); // by-key lookup
    
    println!("DONE!");
    // sleep "forever"
    thread::sleep(time::Duration::from_millis(200_000_000));
}