#[macro_use]
extern crate nom;
extern crate jemallocator;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use nom::{AsBytes, Err, ErrorKind, IResult, Offset, ParseError};
use nom::{alphanumeric, recognize_float, take_while, delimitedc, char, tag, precededc, separated_listc, terminatedc, alt};
use std::str;
use std::iter::repeat;
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum JsonValue {
  Str(String),
  Boolean(bool),
  Num(f64),
  Array(Vec<JsonValue>),
  Object(HashMap<String, JsonValue>),
}

fn sp<'a, E: ParseError<&'a str>>(i: &'a str) ->IResult<&'a str, &'a str, E> {
  let chars = " \t\r\n";

  take_while(move |c| chars.contains(c))(i)
}

fn float<'a, E: ParseError<&'a str>>(i: &'a str) ->IResult<&'a str, f64, E> {
  flat_map!(i, recognize_float, parse_to!(f64))
}

fn parse_str<'a, E: ParseError<&'a str>>(i: &'a str) ->IResult<&'a str, &'a str, E> {
    escaped!(i, call!(alphanumeric), '\\', one_of!("\"n\\"))
}

fn string<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, &'a str, E> {
  //delimitedc(i, char('\"'), parse_str, char('\"'))
  let (i, _) = char('\"')(i)?;

  ctx("string", |i| terminatedc(i, parse_str, char('\"')))(i)
}


fn boolean<'a, E: ParseError<&'a str>>(input: &'a str) ->IResult<&'a str, bool, E> {
  alt( (
      |i| tag("false")(i).map(|(i,_)| (i, false)),
      |i| tag("true")(i).map(|(i,_)| (i, true))
  ))(input)
  /*
  match tag::<&'static str, &'a str, E>("false")(i) {
    Ok((i, _)) => Ok((i, false)),
    Err(_) => tag("true")(i).map(|(i,_)| (i, true))
  }
  */
}

fn array<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, Vec<JsonValue>, E> {
  let (i, _) = char('[')(i)?;

  ctx(
    "array",
    |i| terminatedc(i,
      |i| separated_listc(i, |i| precededc(i, sp, char(',')), value),
      |i| precededc(i, sp, char(']')))
     )(i)
}

fn key_value<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, (&'a str, JsonValue), E> {
  separated_pair!(i, preceded!(sp, string), preceded!(sp, char!(':')), value)
}

fn hash<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, HashMap<String, JsonValue>, E> {
  let (i, _) = char('{')(i)?;
  ctx(
    "map",
    |i| terminatedc(i,
      |i| map!(i,
        separated_list!(preceded!(sp, char!(',')), key_value),
        |tuple_vec| tuple_vec
          .into_iter()
          .map(|(k, v)| (String::from(k), v))
          .collect()
      ),
      |i| precededc(i, sp, char('}')))
     )(i)

/*
  map!(i,
    delimited!(
      char!('{'),
      separated_list!(preceded!(sp, char!(',')), key_value),
      preceded!(sp, char!('}'))
    ),
    |tuple_vec| tuple_vec
      .into_iter()
      .map(|(k, v)| (String::from(k), v))
      .collect()
  )
  */
}

fn value<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, JsonValue, E> {
  preceded!(i,
    sp,
    alt!(
      hash    => { |h| JsonValue::Object(h)            } |
      array   => { |v| JsonValue::Array(v)             } |
      string  => { |s| JsonValue::Str(String::from(s)) } |
      float   => { |f| JsonValue::Num(f)               } |
      boolean => { |b| JsonValue::Boolean(b)           }
    ))
}

fn root<'a, E: ParseError<&'a str>+Ctx<&'a str>>(i: &'a str) ->IResult<&'a str, JsonValue, E> {
  delimited!(i,
    sp,
    alt( (
      |input| hash(input).map(|(i,h)| (i, JsonValue::Object(h))),
      |input| array(input).map(|(i,v)| (i, JsonValue::Array(v)))
    ) ),
    /*alt!(
      hash    => { |h| JsonValue::Object(h)            } |
      array   => { |v| JsonValue::Array(v)             }
    ),*/
    not!(complete!(sp)))
}

#[derive(Clone,Debug,PartialEq)]
struct VerboseError<'a> {
  errors: Vec<(&'a str, VerboseErrorKind)>,
}

#[derive(Clone,Debug,PartialEq)]
pub enum VerboseErrorKind {
  Context(&'static str),
  Char(char),
  //Tag(String),
  Nom(ErrorKind),
}

impl<'a> ParseError<&'a str> for VerboseError<'a> {
  fn from_error_kind(input: &'a str, kind: ErrorKind) -> Self {
    VerboseError {
      errors: vec![(input, VerboseErrorKind::Nom(kind))]
    }
  }

  fn append(input: &'a str, kind: ErrorKind, mut other: Self) -> Self {
    other.errors.push((input, VerboseErrorKind::Nom(kind)));
    other
  }

  fn from_char(input: &'a str, c: char) -> Self {
    VerboseError {
      errors: vec![(input, VerboseErrorKind::Char(c))]
    }
  }

  /*fn from_tag<T:AsBytes>(input: &'a str, t: T) -> Self {
    VerboseError {
      errors: vec![(input, VerboseErrorKind::Char(c))]
    }
  }*/
}

trait Ctx<I> {
  fn add_context(input: I, ctx: &'static str, other: Self) -> Self;
}

impl<I> Ctx<I> for (I, ErrorKind) {
  fn add_context(input: I, ctx: &'static str, other: Self) -> Self {
    other
  }
}

impl<'a> Ctx<&'a str> for VerboseError<'a> {
  fn add_context(input: &'a str, ctx: &'static str, mut other: Self) -> Self {
    other.errors.push((input, VerboseErrorKind::Context(ctx)));
    other
  }
}

fn ctx<'a, E: ParseError<&'a str>+Ctx<&'a str>, F, O>(context: &'static str, f: F) -> impl FnOnce(&'a str) -> IResult<&'a str, O, E>
where
  F: Fn(&'a str) -> IResult<&'a str, O, E> {

    move |i: &'a str| {
      match f(i) {
        Ok(o) => Ok(o),
        Err(Err::Incomplete(i)) => Err(Err::Incomplete(i)),
        Err(Err::Error(e)) | Err(Err::Failure(e)) => {
          Err(Err::Failure(E::add_context(i, context, e)))
        }
      }
    }

}

fn convert_error(input: &str, e: VerboseError) -> String {
  let lines: Vec<_> = input.lines().map(String::from).collect();
  //println!("lines: {:#?}", lines);

  let mut result = String::new();

  for (i, (substring, kind)) in e.errors.iter().enumerate() {
    let mut offset = input.offset(substring);

    let mut line = 0;
    let mut column = 0;

    for (j,l) in lines.iter().enumerate() {
      if offset <= l.len() {
        line = j;
        column = offset;
        break;
      } else {
        offset = offset - l.len();
      }
    }


    match kind {
      VerboseErrorKind::Char(c) => {
        result += &format!("{}: at line {}:\n", i, line);
        result += &lines[line];
        result += "\n";
        if column > 0 {
          result += &repeat(' ').take(column-1).collect::<String>();
        }
        result += "^\n";
        result += &format!("expected '{}', found {}\n\n", c, substring.chars().next().unwrap());
      },
      VerboseErrorKind::Context(s) => {
        result += &format!("{}: at line {}, in {}:\n", i, line, s);
        result += &lines[line];
        result += "\n";
        if column > 0 {
          result += &repeat(' ').take(column -1).collect::<String>();
        }
        result += "^\n\n";
      }
      _ => {}
    }
  }

  result
}

fn main() {
  let data = "  { \"a\"\t: 42,
  \"b\": [ \"x\", \"y\", 12 ] ,
  \"c\": { 1\"hello\" : \"world\"
  }
  } ";

  println!("will try to parse:\n\n**********\n{}\n**********\n", data);
  println!("basic errors - `root::<(&str, ErrorKind)>(data)`:\n{:#?}\n", root::<(&str, ErrorKind)>(data));
  println!("parsed verbose: {:#?}", root::<VerboseError>(data));

  match root::<VerboseError>(data) {
    Err(Err::Error(e)) | Err(Err::Failure(e)) => {
      println!("verbose errors - `root::<VerboseError>(data)`:\n{}", convert_error(data, e));
    },
    _ => panic!(),
  }
}
