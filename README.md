# nys

A declarative (regex-like) parser generator based on `syn` and `quote`. 

## Basic syntax

```
# '?' tells that this construct doesn't have to capture something no matter what
# It is implicitly enabled in SEQ constructs
raw_receptacle = var ( '|' var )* '?'?

# '@' means that result will be pushed to a vec on success
var = '@'? var_name

# Note that var_name should point to a struct field of type
# `dyn syn::parse::Parse`
# or `Option<dyn syn::parse::Parse>`
# or `Vec<dyn syn::parse::Parse>`
# depending on the context
var_name = <syn::Ident>

receptacle = '#' '<' raw_receptacle '>'

# Tells the struct to implement `syn::parse::Parse` trait for
for_data = <syn::Ident>
for_construct = '#' '<' 'FOR' ':' for_data '>'

# Tries to match its raw_receptacle as long as it can
seq_construct = '#' '<' 'SEQ' ':' raw_receptacle '>'

construct = for_construct | seq_construct

# token is literally any token other than receptacle or construct
template = token* (( receptacle | construct )+ token*)*
```

## Usage

First, define a struct into which you will put parsed data.

```rust
pub struct ThingDef {
  // optional `pub` token
  pub pub_token: Option<syn::Token![pub]>,
  // Name of our thing
  pub thing_name: syn::Ident,
}
```

Now let's implement `syn::parse::Parse` for it with help of `nys` library.

```rust
nys::quote_template! {
  #<FOR: ThingDef>
  #<pub_token?> thing #<thing_name>;
}
```

Now you can use the `ThingDef` everywhere where a `syn::pares::Parse`-implementing struct
is needed.

Even in other `nys::quote_template!`s.

Additionally `ThingDef` now has a `nys_parse_stream(s: ::proc_macro::TokenStream) -> ThingDef` function.
Note that it WILL panic on error.