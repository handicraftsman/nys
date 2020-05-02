use std::collections::HashSet;

extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, Span, Delimiter, Punct};
use quote::{quote};
use syn::{
  Token, Ident, Label, Lifetime,
  Result as ParseResult
};
use syn::parse::{Parse, ParseStream};
use syn::parse::discouraged::Speculative;

struct ReceptacleVar {
  name: Ident,
  vec: bool
}

struct Receptacle {
  opt: bool,
  vars: Vec<ReceptacleVar>
}

enum Construct {
  For(Ident),
  Seq(Receptacle)
}

struct QuotedGenerator {
  has_tokens: bool,
  defs_stream: TokenStream2,
  read_stream: TokenStream2,
  base_read_stream: TokenStream2,
  fields_stream: TokenStream2,
  data_class: Option<Ident>,
  ln: i32,
  defs: HashSet<String>,
  current_read_stream: TokenStream2,
  iname: Vec<String>,
}

impl QuotedGenerator {
  fn flush_read_stream(&mut self) {
    if self.has_tokens {
      self.has_tokens = false;
      let mut s = TokenStream2::new();
      std::mem::swap(&mut self.read_stream, &mut s);
      let t = format!("We want to see: {}", s.clone().to_string());
      self.base_read_stream.extend(quote! {
        {
          let _ = #t;
          let q = ::quote::quote! {
            #s
          };
          for ptt in q {
            let res = input_fork.step(|cur| {
              let mut rest = *cur;
              if let Some((tt, next)) = rest.token_tree() {
                let ptts = ::proc_macro2::TokenStream::from(ptt);
                let tts = ::proc_macro2::TokenStream::from(tt);
                if format!("{}", ptts) == format!("{}", tts) {
                  Ok((true, next))
                } else {
                  Err(::syn::Error::new(::proc_macro2::Span::call_site(), format!("Expected `{}`, got `{}`", ptts, tts)))
                }
              } else {
                Ok((false, rest))
              }
            });
            match res {
              Ok(_) => {},
              Err(e) => panic!(e.to_string())
            };
          }
        }
      });
    }
  }

  fn parse_var(&mut self, input: ParseStream, needs_sep: bool) -> ParseResult<ReceptacleVar> {
    let input_fork = input.fork();
    if needs_sep {
      let _ = input_fork.parse::<Token![|]>()?;
    }
    let vec = if let Ok(_) = input_fork.parse::<Token![@]>() {
      true
    } else {
      false
    };
    let name = input_fork.parse::<Ident>()?;
    input.advance_to(&input_fork);
    Ok(ReceptacleVar {
      name,
      vec
    })
  }

  fn parse_vars(&mut self, input: ParseStream, mut first: bool) -> Vec<ReceptacleVar> {
    let mut vars = vec![];

    loop {
      let needs_sep = if !first {
        true
      } else {
        first = false;
        false
      };
      if let Ok(v) = self.parse_var(input, needs_sep) {
        vars.push(v);
      } else {
        break;
      }
    }

    vars
  }

  fn parse_raw_receptacle(&mut self, input: ParseStream) -> ParseResult<Receptacle> {
    let input_fork = input.fork();
    let vars = self.parse_vars(&input_fork, true);
    let opt = if let Ok(_) = input_fork.parse::<Token![?]>() {
      true
    } else {
      false
    };
    input.advance_to(&input_fork);
    Ok(Receptacle {
      opt,
      vars
    })
  }

  fn parse_receptacle(&mut self, input: ParseStream) -> ParseResult<Receptacle> {
    let input_fork = input.fork();
    let _ = input_fork.parse::<Token![#]>()?;
    let _ = input_fork.parse::<Token![<]>()?;
    let receptacle = self.parse_raw_receptacle(&input_fork)?;
    let _ = input_fork.parse::<Token![>]>()?;
    input.advance_to(&input_fork);
    Ok(receptacle)
  }

  fn parse_colon(&mut self, input: ParseStream) -> ParseResult<()> {
    let p = input.parse::<Punct>()?;
    if p.as_char() == ':' {
      Ok(())
    } else {
      Err(input.error("Expected :"))
    }
  }

  fn parse_construct(&mut self, input: ParseStream) -> ParseResult<Construct> {
    let input_fork = input.fork();
    let _ = input_fork.parse::<Token![#]>()?;
    let _ = input_fork.parse::<Token![<]>()?;
    let start = input_fork.parse::<Ident>()?.to_string();
    if start == "FOR" {
      self.parse_colon(&input_fork)?;
      let name = input_fork.parse::<Ident>()?;
      let _ = input_fork.parse::<Token![>]>()?;
      input.advance_to(&input_fork);
      return Ok(Construct::For(name));
    } else if start == "SEQ" {
      self.parse_colon(&input_fork)?;
      let r = self.parse_raw_receptacle(&input_fork)?;
      let _ = input_fork.parse::<Token![>]>()?;
      input.advance_to(&input_fork);
      return Ok(Construct::Seq(r));
    } else {
      return Err(input.error(format!("Invalid construct: {}", start)));
    }
  }

  fn flush_receptacle(&mut self, r: Receptacle) {
    let input = Ident::new(
      self.iname.last().unwrap(),
      Span::call_site()
    );

    let mut first = true;
    for v in &r.vars {
      let e = if !first {
        Some(Token![else](Span::call_site()))
      } else {
        first = true;
        None
      };
      let n = &v.name;

      if v.vec {
        let ns = n.to_string();
        if !self.defs.contains(&ns) {
          self.defs_stream.extend(quote! {
            let mut #n: Vec<_> = vec![];
          });
          self.defs.insert(ns);
        }
      } else {
        self.defs_stream.extend(quote! {
          let mut #n: Option<_> = None;
        });
      }

      if v.vec {
        self.current_read_stream.extend(quote! {
          #e if let Ok(_data) = #input.parse() {
            #n.push(_data);
          }
        });
        self.fields_stream.extend(quote! {
          #n,
        });
      } else if r.vars.len() == 1 {
        if r.opt {
          self.current_read_stream.extend(quote! {
            #n = if let Ok(d) = #input.parse() {
              Some(d)
            } else {
              None
            };
          });
          self.fields_stream.extend(quote! {
            #n,
          });
        } else {
          self.current_read_stream.extend(quote! {
            #n = Some(#input.parse()?);
          });
          self.fields_stream.extend(quote! {
            #n: #n.unwrap(),
          });
        }
      } else {
        self.current_read_stream.extend(quote! {
          #e if let Ok(_data) = #input.parse() {
            #n = Some(_data);
          }
        });
        self.fields_stream.extend(quote! {
          #n,
        });
      }
    }
    if r.vars.len() > 1 && !r.opt {
      self.current_read_stream.extend(quote! {
        else {
          return Err(#input.error("expected token"));
        }
      });
    }
  }

  fn flush_receptacle_seq(&mut self, r: Receptacle) {
    let input = Ident::new(
      self.iname.last().unwrap(),
      Span::call_site()
    );
    if r.opt {
      self.current_read_stream.extend(quote! {
        compile_error("You don't need ? in #<SEQ: ...>")
      });
    }

    let mut first = true;
    let l = syn::parse_str::<Label>(&format!("'l{}:", self.ln)).unwrap();
    let ll = syn::parse_str::<Lifetime>(&format!("'l{}", self.ln)).unwrap();
    self.ln += 1;

    let mut rs = TokenStream2::new();

    for v in &r.vars {
      let e = if !first {
        Some(Token![else](Span::call_site()))
      } else {
        first = true;
        None
      };
      let n = &v.name;

      if v.vec {
        let ns = n.to_string();
        if !self.defs.contains(&ns) {
          self.defs_stream.extend(quote! {
            let mut #n: Vec<_> = vec![];
          });
          self.defs.insert(ns);
        }
      } else {
        self.defs_stream.extend(quote! {
          let mut #n: Option<_> = None;
        });
      }

      if v.vec {
        rs.extend(quote! {
          #e if let Ok(_data) = _fork.parse() {
            #n.push(_data);
          }
        });
        self.fields_stream.extend(quote! {
          #n,
        });
      } else if r.vars.len() == 1 {
        rs.extend(quote! {
          #n = if let Ok(_data) = _fork.parse() {
            Some(_data)
          } else {
            break #ll;
          };
        });
        self.fields_stream.extend(quote! {
          #n: #n.unwrap(),
        });
      } else {
        rs.extend(quote! {
          #e if let Ok(_data) = _fork.parse() {
            #n = Some(_data);
          }
        });
        self.fields_stream.extend(quote! {
          #n,
        });
      }
    }
    if r.vars.len() > 1 {
      rs.extend(quote! {
        else {
          break #ll;
        }
      });
    }

    self.current_read_stream.extend(quote! {
      #l loop {
        let _fork = #input.fork();
        #rs
        #input.advance_to(&_fork);
      }
    });
  }

  fn flush_construct(&mut self, c: Construct) {
    match c {
      Construct::For(n) => {
        self.data_class = Some(n);
      },
      Construct::Seq(r) => {
        self.flush_receptacle_seq(r);
      },
    }
  }

  fn parse_step<'a>(&mut self, input: ParseStream, flush: bool) -> ParseResult<bool> {
    if input.is_empty() {
      return Ok(false)
    }
    let r = if let Ok(r) = self.parse_receptacle(input) {
      self.flush_read_stream();
      self.flush_receptacle(r);
      Ok(true)
    } else if let Ok(c) = self.parse_construct(input) {
      self.flush_read_stream();
      self.flush_construct(c);
      Ok(true)
    } else {
      self.flush_read_stream();
      input.step(|cur| {
        let mut rest = *cur;
        if let Some((c, _, next)) = rest.group(Delimiter::Brace) {
          use syn::parse::Parser;
          (|s: ParseStream| -> ParseResult<()> {
            self.iname.push("input".to_string());
            loop {
              if let Ok(true) = self.parse_next(&s, false) {} else { break; }
            }
            self.iname.pop();
            let mut st = TokenStream2::new();
            std::mem::swap(&mut self.current_read_stream, &mut st);
            self.base_read_stream.extend(quote! {
              {
                use ::syn::parse::Parser;
                let input: ::syn::parse::ParseBuffer;
                let _ = ::syn::braced!(input in input_fork);
                #st
                if !input.is_empty() {
                  return Err(input.error("unexpected token"));
                }
              }
            });
            Ok(())
          }).parse2(c.token_stream())?;
          Ok((true, next))
        } else if let Some((tt, next)) = rest.token_tree() {
          self.has_tokens = true;
          self.read_stream.extend(Into::<TokenStream2>::into(tt));
          rest = next;
          Ok((true, rest))
        } else {
          Ok((false, rest))
        }
      })
    };
    if flush {
      let mut st = TokenStream2::new();
      std::mem::swap(&mut self.current_read_stream, &mut st);
      self.base_read_stream.extend(quote! {
        #st
      });
    }
    r
  }
  
  fn parse_next(&mut self, input: ParseStream, flush: bool) -> ParseResult<bool> {
    let input_fork = input.fork();
    if let Ok(true) = self.parse_step(&input_fork, flush) {
      input.advance_to(&input_fork);
      Ok(true)
    } else {
      Ok(false)
    }
  }
}

impl Parse for QuotedGenerator {  
  fn parse(input: ParseStream) -> ParseResult<Self> {
    let mut s = QuotedGenerator {
      has_tokens: false,
      defs_stream: TokenStream2::new(),
      read_stream: TokenStream2::new(),
      base_read_stream: TokenStream2::new(),
      fields_stream: TokenStream2::new(),
      data_class: None,
      ln: 0,
      defs: HashSet::new(),
      current_read_stream: TokenStream2::new(),
      iname: vec! [ "input_fork".to_string() ]
    };

    loop {
      if let Ok(true) = s.parse_next(input, true) {} else { break; }
    }
    s.flush_read_stream();

    Ok(s)
  }
}

#[proc_macro]
pub fn quote_template(input: TokenStream) -> TokenStream {
  let mut stream = TokenStream2::new();
  let qg = syn::parse_macro_input!(input as QuotedGenerator);

  let n = qg.data_class.expect("No #<FOR: ...> construct found");
  let defs_stream = qg.defs_stream;
  let base_read_stream = qg.base_read_stream;
  let fields_stream = qg.fields_stream;

  stream.extend(quote! {
    impl ::syn::parse::Parse for #n {
      #[allow(unused_assignments)]
      fn parse(input: ::syn::parse::ParseStream) -> syn::Result<Self> {
        use ::syn::parse::discouraged::Speculative;
        let input_fork = input.fork();
        #defs_stream
        #base_read_stream
        input.advance_to(&input_fork);
        Ok(#n {
          #fields_stream
        })
      }
    }

    impl #n {
      pub fn nys_parse_stream(input: ::proc_macro::TokenStream) -> #n {
        match ::syn::parse_macro_input::parse::<#n>(input) {
          ::syn::export::Ok(data) => data,
          ::syn::export::Err(err) => panic!(err.to_string())
        }
      }
    }
  });

  stream.into()
}