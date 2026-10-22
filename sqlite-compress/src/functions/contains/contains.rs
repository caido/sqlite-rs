use memchr::memmem::Finder;
use regex_automata::{
    Anchored,
    hybrid::dfa::{Cache, DFA},
    nfa::thompson,
    util::{start, syntax},
};
use sqlite_ffi::SqliteBlob;
use sqlite_ffi::{Context, SqliteError, Value};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{self, BufReader, Read};
use zstd::stream::read::Decoder;

use crate::{
    DictError, DictId,
    cache::get_decoder_in_cache,
    functions::{ParsedBlob, header::Header},
    state::ExtensionState,
};

const STREAM_CHUNK: usize = 16 * 1024;




