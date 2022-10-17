use crate::crypto_helpers::{eddsa_sign, get_pkh, get_private_key, get_pubkey, get_pubkey_from_privkey, Hasher, Hash, PKH};
use crate::interface::*;
use crate::*;
use arrayvec::ArrayString;
use arrayvec::ArrayVec;
use core::fmt::Write;
use ledger_log::{info};
use ledger_parser_combinators::interp_parser::{
    Action, DefaultInterp, DropInterp, InterpParser, ObserveLengthedBytes, SubInterp, OOB, set_from_thunk
};
use ledger_parser_combinators::json::Json;
use ledger_parser_combinators::core_parsers::Alt;
use ledger_prompts_ui::{write_scroller, final_accept_prompt, mk_prompt_write};

use ledger_parser_combinators::define_json_struct_interp;
use ledger_parser_combinators::json::*;
use ledger_parser_combinators::json_interp::*;
use ledger_parser_combinators::interp_parser::*;
use core::convert::TryFrom;
use core::str::from_utf8;
use nanos_sdk::bindings::*;

// A couple type ascription functions to help the compiler along.
const fn mkfn<A,B>(q: fn(&A,&mut B)->Option<()>) -> fn(&A,&mut B)->Option<()> {
  q
}
const fn mkmvfn<A,B,C>(q: fn(A,&mut B)->Option<C>) -> fn(A,&mut B)->Option<C> {
    q
}
const fn mkfnc<A,B,C>(q: fn(&A,&mut B,C)->Option<()>) -> fn(&A,&mut B,C)->Option<()> {
    q
}
const fn mkvfn<A>(q: fn(&A,&mut Option<()>)->Option<()>) -> fn(&A,&mut Option<()>)->Option<()> {
  q
}

pub type GetAddressImplT = impl InterpParser<Bip32Key, Returning = ArrayVec<u8, 128_usize>>;
pub const GET_ADDRESS_IMPL: GetAddressImplT =
    Action(SubInterp(DefaultInterp), mkfn(|path: &ArrayVec<u32, 10>, destination: &mut Option<ArrayVec<u8, 128>>| {
        let key = get_pubkey(&path).ok()?;

        let pkh = get_pkh(key);

        write_scroller("Provide Public Key", |w| Ok(write!(w, "{}", pkh)?))?;

        final_accept_prompt(&[])?;

        *destination=Some(ArrayVec::new());
        // key without y parity
        let key_x = &key.W[1..key.W_len as usize];
        destination.as_mut()?.try_push(u8::try_from(key_x.len()).ok()?).ok()?;
        destination.as_mut()?.try_extend_from_slice(key_x).ok()?;
        Some(())
    }));

pub type SignImplT = impl InterpParser<SignParameters, Returning = ArrayVec<u8, 128_usize>>;

#[derive(PartialEq, Debug)]
enum CapabilityCoverage {
    Full,
    HasFallback,
    NoCaps
}

impl Summable<CapabilityCoverage> for CapabilityCoverage {
    fn zero() -> Self { CapabilityCoverage::Full }
    fn add_and_set(&mut self, other: &CapabilityCoverage) {
        match other {
            CapabilityCoverage::Full => { }
            CapabilityCoverage::HasFallback => { if *self == CapabilityCoverage::Full { *self = CapabilityCoverage::HasFallback } }
            CapabilityCoverage::NoCaps => { *self = CapabilityCoverage::NoCaps }
        }
    }
}

pub static SIGN_IMPL: SignImplT = Action(
    (
        Action(
            // Calculate the hash of the transaction
            ObserveLengthedBytes(
                Hasher::new,
                Hasher::update,
                Json(Action(Preaction( || -> Option<()> { write_scroller("Signing", |w| Ok(write!(w, "Transaction")?)) } , KadenaCmdInterp {
                    field_nonce: DropInterp,
                    field_meta: META_ACTION,
                    field_payload: PayloadInterp {
                        field_exec: CommandInterp {
                            field_code: DropInterp,
                            field_data: DropInterp
                        }},
                    field_signers: SubInterpM::<_, CapabilityCoverage>::new(Action(Preaction(
                            || -> Option<()> {
                                write_scroller("Requiring", |w| Ok(write!(w, "Capabilities")?))
                            },
                            SignerInterp {
                        field_scheme: DropInterp,
                        field_pub_key: MoveAction(JsonStringAccumulate::<64>, mkmvfn(|key : ArrayVec<u8, 64>, dest: &mut Option<ArrayVec<u8, 64>>| -> Option<()> {
                            write_scroller("Of Key", |w| Ok(write!(w, "{}", from_utf8(key.as_slice())?)?))?;
                            set_from_thunk(dest, || Some(key));
                            Some(())
                        })),
                        field_addr: DropInterp,
                        field_clist: Alt(DropInterp, CLIST_ACTION),
                    }),
                        mkfn(|signer: &Signer<_,Option<ArrayVec<u8, 64>>,_, Option<AltResult<(),(CapCountData, All)>>>, dest: &mut Option<CapabilityCoverage> | {
                            *dest = Some(match signer.field_clist {
                                Some(AltResult::Second((CapCountData::CapCount{total_caps,..}, All(a)))) if total_caps > 0 => if a {CapabilityCoverage::Full} else {CapabilityCoverage::HasFallback},
                                _ => {
                                    match from_utf8(signer.field_pub_key.as_ref()?.as_slice()) {
                                        Ok(pub_key) => write_scroller("Unscoped Signer", |w| Ok(write!(w, "{}", pub_key)?)),
                                        _ => Some(()),
                                    };
                                    CapabilityCoverage::NoCaps
                                },
                            });
                            Some(())
                        })),
                        ),
                    field_network_id: Action(Alt(JsonStringAccumulate::<32>, DropInterp), mkvfn(|mnet: &AltResult<ArrayVec<u8, 32>, ()>, dest: &mut Option<()>| {
                        *dest = Some(());
                        match mnet {
                            AltResult::First(net) => {
                                write_scroller("On Network", |w| Ok(write!(w, "{}", from_utf8(net.as_slice())?)?))
                            }
                            _ => { Some(())} // Ignore null
                        }
                    }))
                }),
                mkvfn(|cmd : &KadenaCmd<_,_,Option<CapabilityCoverage>,_,_>, _| { 
                    match cmd.field_signers.as_ref() {
                        Some(CapabilityCoverage::Full) => { }
                        Some(CapabilityCoverage::HasFallback) => {
                            write_scroller("WARNING", |w| Ok(write!(w, "Transaction too large for Ledger to display.  PROCEED WITH GREAT CAUTION.  Do you want to continue?")?))?;
                        }
                        _ => {
                            write_scroller("WARNING", |w| Ok(write!(w, "UNSAFE TRANSACTION. This transaction's code was not recognized and does not limit capabilities for all signers. Signing this transaction may make arbitrary actions on the chain including loss of all funds.")?))?;
                        }
                    }
                    Some(())
                })
                )),
            true),
            // Ask the user if they accept the transaction body's hash
            mkfn(|(_, mut hash): &(_, Hasher), destination: &mut Option<[u8; 32]>| {
                let the_hash = hash.finalize();
                write_scroller("Transaction hash", |w| Ok(write!(w, "{}", the_hash)?))?;
                *destination=Some(the_hash.0.into());
                Some(())
            }),
        ),
        Action(
            SubInterp(DefaultInterp),
            // And ask the user if this is the key the meant to sign with:
            mkfn(|path: &ArrayVec<u32, 10>, destination: &mut _| {
                // Mutable because of some awkwardness with the C api.
                let mut privkey = get_private_key(&path).ok()?;
                let pubkey = get_pubkey_from_privkey(&mut privkey).ok()?;
                let pkh = get_pkh(pubkey);

                write_scroller("Sign for Address", |w| Ok(write!(w, "{}", pkh)?))?;
                *destination = Some(privkey);
                Some(())
            }),
        ),
    ),
    mkfn(|(hash, key): &(Option<[u8; 32]>, Option<_>), destination: &mut _| {
        final_accept_prompt(&[&"Sign Transaction?"])?;

        // By the time we get here, we've approved and just need to do the signature.
        let sig = eddsa_sign(&hash.as_ref()?[..], key.as_ref()?)?;
        let mut rv = ArrayVec::<u8, 128>::new();
        rv.try_extend_from_slice(&sig.0[..]).ok()?;
        *destination = Some(rv);
        Some(())
    }),
);

const META_ACTION:
  Action<Alt<MetaInterp<
             Action<JsonStringAccumulate<32_usize>, fn(&ArrayVec<u8, 32_usize>, &mut Option<()>) -> Option<()>>
             , DropInterp
             , JsonStringAccumulate<100_usize>
             , JsonStringAccumulate<100_usize>
             , DropInterp
             , DropInterp>
          , DropInterp>
         , fn(&AltResult<Meta<Option<()>, Option<()>, Option<ArrayVec<u8, 100_usize>>
                    , Option<ArrayVec<u8, 100_usize>>, Option<()>, Option<()>>, ()>
              , &mut Option<()>) -> Option<()>
         >
    = Action(
        Alt(MetaInterp {
            field_chain_id: Action(JsonStringAccumulate::<32>, mkvfn(|chain: &ArrayVec<u8, 32>, _| -> Option<()> {
                write_scroller("On Chain", |w| Ok(write!(w, "{}", from_utf8(chain.as_slice()).ok()?)?))
            })),
            field_sender: DropInterp,
            field_gas_limit: JsonStringAccumulate::<100>,
            field_gas_price: JsonStringAccumulate::<100>,
            field_ttl: DropInterp,
            field_creation_time: DropInterp
        }, DropInterp)
        , mkvfn(|v , _| {
            match v {
                AltResult::First(Meta { ref field_gas_limit, ref field_gas_price, .. }) => {
                    write_scroller("Using Gas", |w| Ok(write!(w, "at most {} at price {}"
                      , from_utf8(field_gas_limit.as_ref()?.as_slice()).ok()?
                      , from_utf8(field_gas_price.as_ref()?.as_slice()).ok()?)?))
                }
                _ => {
                    write_scroller("CAUTION", |w| Ok(write!(w, "'meta' field of transaction not recognized")?))
                }
            }
        }));

#[derive(Debug, Clone, Copy)]
enum CapCountData {
    IsTransfer,
    IsUnknownCap,
    CapCount {
        total_caps: u16,
        total_transfers: u16,
        total_unknown: u16,
    },
}

impl Summable<CapCountData> for CapCountData {
    fn add_and_set(&mut self, other: &CapCountData) {
        match self {
            CapCountData::CapCount {total_caps, total_transfers, total_unknown} => {
                *total_caps += 1;
                match other {
                    CapCountData::IsTransfer => *total_transfers += 1,
                    CapCountData::IsUnknownCap => *total_unknown += 1,
                    _ => {},
                }
            },
            _ => {}
        }
    }
    fn zero() -> Self { CapCountData::CapCount { total_caps: 0, total_transfers: 0, total_unknown: 0} }
}

const CLIST_ACTION:
  SubInterpMFold::<
    Action< KadenaCapabilityInterp<KadenaCapabilityArgsInterp, JsonStringAccumulate<128_usize>>
          , fn( &KadenaCapability< Option<<KadenaCapabilityArgsInterp as ParserCommon<JsonArray<JsonAny>>>::Returning>
                                , Option<ArrayVec<u8, 128_usize>>>
              , &mut Option<(CapCountData, bool)>
              , (CapCountData, All)
              ) -> Option<()>
          >
    , (CapCountData, All)
    > =
  SubInterpMFold::new(Action(
      KadenaCapabilityInterp {
          field_args: KadenaCapabilityArgsInterp,
          field_name: JsonStringAccumulate::<128>
      },
      mkfnc(|cap : &KadenaCapability<Option<<KadenaCapabilityArgsInterp as ParserCommon<JsonArray<JsonAny>>>::Returning>, Option<ArrayVec<u8, 128>>>, destination: &mut Option<(CapCountData, bool)>, v: (CapCountData, All)| {
          let name = cap.field_name.as_ref()?.as_slice();
          let name_utf8 = from_utf8(name).ok()?;
          let mk_unknown_cap_title = || -> Option <_>{
              let count = match v.0 {
                  CapCountData::CapCount{ total_unknown, ..} => total_unknown,
                  _ => 0,
              };
              let mut buffer: ArrayString<22> = ArrayString::new();
              Ok(write!(mk_prompt_write(&mut buffer), "Unknown Capability {}", count + 1).ok()?)?;
              Some(buffer)
          };
          let mk_transfer_title = || -> Option <_>{
              let count = match v.0 {
                  CapCountData::CapCount{ total_transfers, ..} => total_transfers,
                  _ => 0,
              };
              let mut buffer: ArrayString<22> = ArrayString::new();
              Ok(write!(mk_prompt_write(&mut buffer), "Transfer {}", count + 1).ok()?)?;
              Some(buffer)
          };

          trace!("Prompting for capability");
          *destination = Some((CapCountData::IsUnknownCap, true));
          match cap.field_args.as_ref() {
              Some((None, _)) => {
                  if name == b"coin.GAS" {
                      write_scroller("Paying Gas", |w| Ok(write!(w, " ")?))?;
                      *destination = Some((Summable::zero(), true));
                      trace!("Accepted gas");
                  } else {
                      write_scroller(&mk_unknown_cap_title()?, |w| Ok(write!(w, "name: {}, no args", name_utf8)?))?;
                  }
              }
              Some((Some(Some(args)), arg_lengths)) => {
                  if arg_lengths[3] != 0 {
                      write_scroller(&mk_unknown_cap_title()?, |w| Ok(
                          write!(w, "name: {}, arg 1: {}, arg 2: {}, arg 3: {}, arg 4: {}, arg 5: {}", name_utf8
                                 , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                 , from_utf8(args.as_slice().get(arg_lengths[0]..arg_lengths[1])?)?
                                 , from_utf8(args.as_slice().get(arg_lengths[1]..arg_lengths[2])?)?
                                 , from_utf8(args.as_slice().get(arg_lengths[2]..arg_lengths[3])?)?
                                 , from_utf8(args.as_slice().get(arg_lengths[3]..args.len())?)?
                          )?))?;
                  } else if arg_lengths[2] != 0 {
                      if name == b"coin.TRANSFER_XCHAIN" {
                          write_scroller(&mk_transfer_title()?, |w| Ok(
                              write!(w, "Cross-chain {} from {} to {} to chain {}"
                                     , from_utf8(args.as_slice().get(arg_lengths[1]..arg_lengths[2])?)?
                                     , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[0]..arg_lengths[1])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[2]..args.len())?)?
                              )?))?;
                          *destination = Some((CapCountData::IsTransfer, true));
                      } else {
                          write_scroller(&mk_unknown_cap_title()?, |w| Ok(
                              write!(w, "name: {}, arg 1: {}, arg 2: {}, arg 3: {}, arg 4: {}", name_utf8
                                     , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[0]..arg_lengths[1])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[1]..arg_lengths[2])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[2]..args.len())?)?
                              )?))?;
                      }
                  } else if arg_lengths[1] != 0 {
                      if name == b"coin.TRANSFER" {
                          write_scroller(&mk_transfer_title()?, |w| Ok(
                              write!(w, "{} from {} to {}"
                                     , from_utf8(args.as_slice().get(arg_lengths[1]..args.len())?)?
                                     , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[0]..arg_lengths[1])?)?
                              )?))?;
                          *destination = Some((CapCountData::IsTransfer, true));
                      } else {
                          write_scroller(&mk_unknown_cap_title()?, |w| Ok(
                              write!(w, "name: {}, arg 1: {}, arg 2: {}, arg 3: {}", name_utf8
                                     , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[0]..arg_lengths[1])?)?
                                     , from_utf8(args.as_slice().get(arg_lengths[1]..args.len())?)?
                              )?))?;
                      }
                  } else if arg_lengths[0] != 0 {
                      write_scroller(&mk_unknown_cap_title()?, |w| Ok(
                          write!(w, "name: {}, arg 1: {}, arg 2: {}", name_utf8
                                 , from_utf8(args.as_slice().get(0..arg_lengths[0])?)?
                                 , from_utf8(args.as_slice().get(arg_lengths[0]..args.len())?)?
                      )?))?;
                  } else {
                      if name == b"coin.ROTATE" {
                          write_scroller("Rotate for account", |w| Ok(write!(w, "{}", from_utf8(args.as_slice())?)?))?;
                          *destination = Some((Summable::zero(), true));
                      } else {
                          write_scroller(&mk_unknown_cap_title()?, |w| Ok(write!(w, "name: {}, arg 1: {}", name_utf8, from_utf8(args.as_slice())?)?))?;
                      }
                  }
              }
              _ => {
                  write_scroller(&mk_unknown_cap_title()?, |w| Ok(write!(w, "name: {}, args cannot be displayed on Ledger", name_utf8)?))?;
                  set_from_thunk(destination, || Some((CapCountData::IsUnknownCap, false))); // Fallback case
              }
          }
          Some(())
      }),
  ));

pub type SignHashImplT = impl InterpParser<SignHashParameters, Returning = ArrayVec<u8, 128_usize>>;

pub static SIGN_HASH_IMPL: SignHashImplT = Action(
    Preaction( || -> Option<()> {
        write_scroller("WARNING", |w| Ok(write!(w, "Blind Signing a Transaction Hash is a very unusual operation. Do not continue unless you know what you are doing")?))
    } ,
    (
        Action(
            SubInterp(DefaultInterp),
            // Ask the user if they accept the transaction body's hash
            mkfn(|hash_val: &[u8; 32], destination: &mut Option<[u8; 32]>| {
                let the_hash = Hash ( *hash_val );
                write_scroller("Transaction hash", |w| Ok(write!(w, "{}", the_hash)?))?;
                *destination=Some(the_hash.0.into());
                Some(())
            }),
        ),
        Action(
            SubInterp(DefaultInterp),
            // And ask the user if this is the key the meant to sign with:
            mkfn(|path: &ArrayVec<u32, 10>, destination: &mut _| {
                // Mutable because of some awkwardness with the C api.
                let mut privkey = get_private_key(&path).ok()?;
                let pubkey = get_pubkey_from_privkey(&mut privkey).ok()?;
                let pkh = get_pkh(pubkey);

                write_scroller("Sign for Address", |w| Ok(write!(w, "{}", pkh)?))?;
                *destination = Some(privkey);
                Some(())
            }),
        ),
    )),
    mkfn(|(hash, key): &(Option<[u8; 32]>, Option<_>), destination: &mut _| {
        final_accept_prompt(&[&"Sign Transaction Hash?"])?;

        // By the time we get here, we've approved and just need to do the signature.
        let sig = eddsa_sign(&hash.as_ref()?[..], key.as_ref()?)?;
        let mut rv = ArrayVec::<u8, 128>::new();
        rv.try_extend_from_slice(&sig.0[..]).ok()?;
        *destination = Some(rv);
        Some(())
    }),
);

pub struct KadenaCapabilityArgsInterp;

// The Caps list is parsed and the args are stored in a single common ArrayVec of this size.
// (This may be as large as the stack allows)
const ARG_ARRAY_SIZE: usize = 184;
const MAX_ARG_COUNT: usize = 5;

// Since we use a single ArrayVec to store the rendered json of all the args.
// This list keeps track of the indices in the array for each arg, and even the args count

// If there are three args; then indices[0] will contain the end of first arg, indices[1] will be end of second, and indices[2] will be 0
// In other words, first arg will be: array[0..indices[0]], second: array[indices[0]..indices[1]], third: array[indices[1]..array.len()]
type ArgListIndicesT = [usize; MAX_ARG_COUNT - 1];

// The Alt parser will first try to parse JsonAny and render it upto the available space in array
// on hitting end of array it will fallback to the OrDropAny
type CapArgT = Alt<JsonAny, JsonAny>;
type CapArgInterpT = OrDropAny<JsonStringAccumulate<ARG_ARRAY_SIZE>>;

#[derive(Debug)]
pub enum KadenaCapabilityArgsInterpState {
    Start,
    Begin,
    Argument(<CapArgInterpT as ParserCommon<CapArgT>>::State),
    ValueSep,
    FallbackValue(<DropInterp as ParserCommon<JsonAny>>::State),
    FallbackValueSep
}

impl ParserCommon<JsonArray<JsonAny>> for KadenaCapabilityArgsInterp {
    type State = (KadenaCapabilityArgsInterpState, Option<<DropInterp as ParserCommon<JsonAny>>::Returning>, usize);
    type Returning = (Option<<CapArgInterpT as ParserCommon<CapArgT>>::Returning>, ArgListIndicesT );
    fn init(&self) -> Self::State {
        (KadenaCapabilityArgsInterpState::Start, None, 0)
    }
}
impl JsonInterp<JsonArray<JsonAny>> for KadenaCapabilityArgsInterp {
    #[inline(never)]
    fn parse<'a, 'b>(&self, (ref mut state, ref mut scratch, ref mut arg_count): &'b mut Self::State, token: JsonToken<'a>, destination: &mut Option<Self::Returning>) -> Result<(), Option<OOB>> {
        let str_interp = OrDropAny(JsonStringAccumulate::<ARG_ARRAY_SIZE>);
        loop {
            use KadenaCapabilityArgsInterpState::*;
            match state {
                Start if token == JsonToken::BeginArray => {
                    set_from_thunk(destination, || Some((None, [0,0,0,0])));
                    set_from_thunk(state, || Begin);
                }
                Begin if token == JsonToken::EndArray => {
                    return Ok(());
                }
                Begin => {
                    set_from_thunk(state, || Argument(<CapArgInterpT as ParserCommon<CapArgT>>::init(&str_interp)));
                    *arg_count = 1;
                    continue;
                }
                Argument(ref mut s) => {
                    <CapArgInterpT as JsonInterp<CapArgT>>::parse(&str_interp, s, token, &mut destination.as_mut().ok_or(Some(OOB::Reject))?.0)?;
                    set_from_thunk(state, || ValueSep);
                }
                ValueSep if token == JsonToken::ValueSeparator => {
                    match &destination.as_mut().ok_or(Some(OOB::Reject))?.0 {
                        Some(Some(sub_dest)) if *arg_count < MAX_ARG_COUNT => {
                            destination.as_mut().ok_or(Some(OOB::Reject))?.1[*arg_count-1] = sub_dest.len();
                            set_from_thunk(state, || Argument(<CapArgInterpT as ParserCommon<CapArgT>>::init(&str_interp)));
                            *arg_count+=1;
                        }
                        _ => {
                            set_from_thunk(destination, || None);
                            set_from_thunk(state, || FallbackValue(<DropInterp as ParserCommon<JsonAny>>::init(&DropInterp)));
                        }
                    }
                }
                ValueSep if token == JsonToken::EndArray => return Ok(()),
                FallbackValue(ref mut s) => {
                    <DropInterp as JsonInterp<JsonAny>>::parse(&DropInterp, s, token, scratch)?;
                    set_from_thunk(state, || FallbackValueSep);
                }
                FallbackValueSep if token == JsonToken::ValueSeparator => {
                    set_from_thunk(state, || FallbackValue(<DropInterp as ParserCommon<JsonAny>>::init(&DropInterp)));
                }
                FallbackValueSep if token == JsonToken::EndArray => {
                    return Ok(());
                }
                _ => return Err(Some(OOB::Reject))
            }
            break Err(None)
        }
    }
}

// ----------------------------------------------------------------------------------

#[inline(never)]
fn handle_first_prompt (
    pkh_str: &ArrayString<64>, hasher: &mut Hasher
        , txType: u8
        , recipient: &ArrayVec<u8, PARAM_RECIPIENT_SIZE>
        , recipient_chain: &ArrayVec<u8, PARAM_RECIPIENT_CHAIN_SIZE>
        , amount: &ArrayVec<u8, PARAM_AMOUNT_SIZE>
        , network: &ArrayVec<u8, PARAM_NETWORK_SIZE>
) -> Option<()>
{
    // TODO: clist amount in decimal
    let amount_str = from_utf8(amount).ok()?;
    let recipient_str = from_utf8(recipient).ok()?;
    let recipient_chain_str = from_utf8(recipient_chain).ok()?;
    let network_str = from_utf8(network).ok()?;
    // let mut buffer: ArrayString<100> = ArrayString::new();
    // let mut pw = mk_prompt_write(&mut buffer);
    // let whash = |hasher: &mut Hasher, buffer:&mut ArrayString<100>| {
    //     info!("PW: {}", buffer.as_str());
    //     info!("PW: {}", buffer.as_str());
    // };
    // curly braces are escaped like '{{', '}}'
    // The JSON struct begins here, and ends in handle_second_prompt
    write!(hasher, "{{").ok()?;
    write!(hasher, "\"networkId\":\"{}\"", network_str).ok()?;
    // let b = buffer.as_ref();
    // hasher.update(b.as_bytes());
    match txType {
        0 => {
            write!(hasher, ",\"payload\":{{\"exec\":{{\"data\":{{}},\"code\":\"").ok()?;
            write!(hasher, "(coin.transfer \\\"k:{}\\\"", pkh_str).ok()?;
            write!(hasher, " \\\"k:{}\\\"", recipient_str).ok()?;
            write!(hasher, " {})\"}}}}", amount_str).ok()?;
            write!(hasher, ",\"signers\":[{{\"pubKey\":").ok()?;
            write!(hasher, "\"{}\"", pkh_str).ok()?;
            write!(hasher, ",\"clist\":[{{\"args\":[").ok()?;
            write!(hasher, "\"k:{}\",", pkh_str).ok()?;
            write!(hasher, "\"k:{}\",", recipient_str).ok()?;
            write!(hasher, "{}]", amount_str).ok()?;
            write!(hasher, ",\"name\":\"coin.TRANSFER\"}},{{\"args\":[],\"name\":\"coin.GAS\"}}]}}]").ok()?;
        },
        1 => {
            write!(hasher, ",\"payload\":{{\"exec\":{{\"data\":{{").ok()?;
            write!(hasher, "\"ks\":{{\"pred\":\"keys-all\",\"keys\":[").ok()?;
            write!(hasher, "\"{}\"]}}}}", recipient_str).ok()?;
            write!(hasher, ",\"code\":\"").ok()?;
            write!(hasher, "(coin.transfer-create \\\"k:{}\\\"", pkh_str).ok()?;
            write!(hasher, " \\\"k:{}\\\"", recipient_str).ok()?;
            write!(hasher, " (read-keyset \\\"ks\\\")").ok()?;
            write!(hasher, " {})\"}}}}", amount_str).ok()?;
            write!(hasher, ",\"signers\":[{{\"pubKey\":").ok()?;
            write!(hasher, "\"{}\"", pkh_str).ok()?;
            write!(hasher, ",\"clist\":[{{\"args\":[").ok()?;
            write!(hasher, "\"k:{}\",", pkh_str).ok()?;
            write!(hasher, "\"k:{}\",", recipient_str).ok()?;
            write!(hasher, "{}]", amount_str).ok()?;
            write!(hasher, ",\"name\":\"coin.TRANSFER\"}},{{\"args\":[],\"name\":\"coin.GAS\"}}]}}]").ok()?;
        },
        2 => {
            write!(hasher, ",\"payload\":{{\"exec\":{{\"data\":{{").ok()?;
            write!(hasher, "\"ks\":{{\"pred\":\"keys-all\",\"keys\":[").ok()?;
            write!(hasher, "\"{}\"]}}}}", recipient_str).ok()?;
            write!(hasher, ",\"code\":\"").ok()?;
            write!(hasher, "(coin.transfer-crosschain \\\"k:{}\\\"", pkh_str).ok()?;
            write!(hasher, " \\\"k:{}\\\"", recipient_str).ok()?;
            write!(hasher, " (read-keyset \\\"ks\\\")").ok()?;
            write!(hasher, " \\\"{}\\\"", recipient_chain_str).ok()?;
            write!(hasher, " {})\"}}}}", amount_str).ok()?;
            write!(hasher, ",\"signers\":[{{\"pubKey\":").ok()?;
            write!(hasher, "\"{}\"", pkh_str).ok()?;
            write!(hasher, ",\"clist\":[{{\"args\":[").ok()?;
            write!(hasher, "\"k:{}\",", pkh_str).ok()?;
            write!(hasher, "\"k:{}\",", recipient_str).ok()?;
            write!(hasher, "{},", amount_str).ok()?;
            write!(hasher, "\"{}\"]", recipient_chain_str).ok()?;
            write!(hasher, ",\"name\":\"coin.TRANSFER_XCHAIN\"}},{{\"args\":[],\"name\":\"coin.GAS\"}}]}}]").ok()?;
        }
        _ => {}
    }
        
    match txType {
        0 | 1 => {
            write_scroller("Transfer", |w| Ok(write!(w, "{} from k:{} to {} on network {}"
              , amount_str, pkh_str, recipient_str, network_str)?))?;
        },
        2 => {
            write_scroller("Transfer", |w| Ok(write!(w, "Cross-chain {} from k:{} to {} to chain {} on network {}"
              , amount_str, pkh_str, recipient_str, recipient_chain_str, network_str)?))?;
        }
        _ => {}
    }
    Some(())
}

fn handle_second_prompt (
    pkh_str: &ArrayString<64>, hasher: &mut Hasher
        , gasPrice: &ArrayVec<u8, PARAM_GAS_PRICE_SIZE>
        , gasLimit: &ArrayVec<u8, PARAM_GAS_LIMIT_SIZE>
        , creationTime: &ArrayVec<u8, PARAM_CREATION_TIME_SIZE>
        , chainId: &ArrayVec<u8, PARAM_CHAIN_SIZE>
        , nonce: &ArrayVec<u8, PARAM_NOONCE_SIZE>
        , ttl: &ArrayVec<u8, PARAM_TTL_SIZE>
) -> Option<()>
{
    write!(hasher, ",\"meta\":{{").ok()?;
    write!(hasher, "\"creationTime\":{}", from_utf8(creationTime).ok()?).ok()?;
    write!(hasher, ",\"ttl\":{}", from_utf8(ttl).ok()?).ok()?;
    write!(hasher, ",\"gasLimit\":{}", from_utf8(gasLimit).ok()?).ok()?;
    write!(hasher, ",\"chainId\":\"{}\"", from_utf8(chainId).ok()?).ok()?;
    write!(hasher, ",\"gasPrice\":{}", from_utf8(gasPrice).ok()?).ok()?;
    write!(hasher, ",\"sender\":\"k:{}\"", pkh_str).ok()?;
    write!(hasher, "}}").ok()?;
    write!(hasher, ",\"nonce\":\"{}\"", from_utf8(nonce).ok()?).ok()?;
    // The JSON struct ends here
    write!(hasher, "}}").ok()?;

    write_scroller("Paying Gas", |w| Ok(write!(w, "at most {} at price {}", from_utf8(gasLimit)?, from_utf8(gasPrice)?)?))?;
    Some(())
}

type HasherAndPath = (Hasher, cx_ecfp_256_private_key_s);
type HasherAndPath2 = (u8, ArrayVec<u32, 10>);

pub type OptionByteVec<const N: usize> = Option<ArrayVec<u8, N>>;

type SubDefT = SubInterp<DefaultInterp>;
const SubDef: SubDefT = SubInterp(DefaultInterp);

pub type PathRecipientAmountT = impl InterpParser<MakeTransferTxParameters1, Returning = HasherAndPath>;

pub type MakeTransferTxParameters1RV = (Option<ArrayVec<u32, 10_usize>>
           , Option<(Option<u8>
           , Option<(OptionByteVec<PARAM_RECIPIENT_SIZE>
           , Option<(OptionByteVec<PARAM_RECIPIENT_CHAIN_SIZE>
           , Option<(OptionByteVec<PARAM_NETWORK_SIZE>
           , OptionByteVec<PARAM_AMOUNT_SIZE>)>)>)>)>);

const PathRecipientAmountP: PathRecipientAmountT
  // : MoveAction
  //   <(SubDefT, (DefaultInterp, (SubDefT, (SubDefT, (SubDefT, SubDefT)))))
  // , fn(MakeTransferTxParameters1RV, &mut Option<HasherAndPath2>) -> Option<()>
  //    >
  = MoveAction(
      ( SubDef, (DefaultInterp, (SubDef, (SubDef, (SubDef, SubDef)))))
    , mkmvfn(|(path, optv1): MakeTransferTxParameters1RV, destination:&mut Option<HasherAndPath>| {
        let mut privkey = get_private_key(&path?).ok()?;

        let mut pkh_str: ArrayString<64> = ArrayString::new();
        {
            let pubkey = get_pubkey_from_privkey(&mut privkey).ok()?;
            let pkh = get_pkh(pubkey);
            Ok(write!(mk_prompt_write(&mut pkh_str), "{}", pkh).ok()?)?;
        }
        let (txType, optv2) = optv1?;
        let (recipient, optv3) = optv2?;
        let (recipient_chain, optv4) = optv3?;
        let (network, amount) = optv4?;
        set_from_thunk(destination, || Some((Hasher::new(), privkey)));
        match destination {
            Some((ref mut hasher, _)) => {
                // Works
                // write_scroller("pkh", |w| Ok(write!(w, " {} ", pkh_str)?))?;
                // write_scroller("Transfer", |w| Ok(write!(w, "{} from k:{} to {} on network {}"
                //   , from_utf8(amount.as_ref()?).ok()?, from_utf8(recipient.as_ref()?).ok()?, from_utf8(recipient.as_ref()?).ok()?, from_utf8(network.as_ref()?).ok()?)?))?;

                // core-dump (w/o debug)

                // write_scroller("pkh", |w| Ok(write!(w, " {} ", pkh)?))?;
                // OR
                // write_scroller("Transfer", |w| Ok(write!(w, "{} from k:{} to {} on network {}"
                //   , from_utf8(amount.as_ref()?).ok()?, pkh_str, from_utf8(recipient.as_ref()?).ok()?, from_utf8(network.as_ref()?).ok()?)?))?;


                // (w/o debug) gets stuck, (likely same hang issue as with debug)
                handle_first_prompt(&pkh_str, hasher, txType?, recipient.as_ref()?, recipient_chain.as_ref()?, amount.as_ref()?, network.as_ref()?)?;

            }
            _ => {}
        }
        Some(())
    }),
    );

pub type MakeTransferTxParameters2RV = (Option<ArrayVec<u8, PARAM_GAS_PRICE_SIZE>>
           , Option<(OptionByteVec<PARAM_GAS_LIMIT_SIZE>
           , Option<(OptionByteVec<PARAM_CREATION_TIME_SIZE>
           , Option<(OptionByteVec<PARAM_CHAIN_SIZE>
           , Option<(OptionByteVec<PARAM_NOONCE_SIZE>
           , OptionByteVec<PARAM_TTL_SIZE>)>)>)>)>);

pub type MetaNonceT = impl InterpParser<MakeTransferTxParameters2, Returning = HasherAndPath>;

const MetaNonceP: MetaNonceT
  // : MoveAction
  //   <(SubDefT, (SubDefT, (SubDefT, (SubDefT, (SubDefT, SubDefT)))))
  // , fn(MakeTransferTxParameters2RV, &mut Option<([u8; 32], ArrayVec<u32, 10>)>) -> Option<()>
  //    >

  = MoveAction(
      ( SubDef, (SubDef, (SubDef, (SubDef, (SubDef, SubDef)))))
   , mkmvfn(|(network, optv1): MakeTransferTxParameters2RV, destination: &mut Option<HasherAndPath>| {
        let (gasPrice, optv2) = optv1?;
        let (gasLimit, optv3) = optv2?;
        let (chainId, optv4) = optv3?;
        let (creationTime, ttl) = optv4?;
        match destination {
            Some((ref mut hasher, ref mut privkey)) => {
                let mut pkh_str: ArrayString<64> = ArrayString::new();
                {
                    let pubkey = get_pubkey_from_privkey(privkey).ok()?;
                    let pkh = get_pkh(pubkey);
                    Ok(write!(mk_prompt_write(&mut pkh_str), "{}", pkh).ok()?)?;
                }
                handle_second_prompt(&pkh_str, hasher, &network?, &gasPrice?, &gasLimit?, &chainId?, &creationTime?, &ttl?)?;
            }
            _ => {
                panic!("destination should have been set")
            }
        }
        Some(())
    }),
    );

pub type MakeTransferTxImplT = impl InterpParser<MakeTransferTxParameters, Returning = ArrayVec<u8, 128_usize>>;

pub struct MakeTx;

pub enum MakeTxSubState {
    Init,
    PathRecipientAmount(<PathRecipientAmountT as ParserCommon<MakeTransferTxParameters1>>::State),
    MetaNonce(<MetaNonceT as ParserCommon<MakeTransferTxParameters2>>::State),
    Done,
}

impl ParserCommon<MakeTransferTxParameters> for MakeTx {
    type State = (Option<HasherAndPath>, MakeTxSubState);
    type Returning = ArrayVec<u8, 128_usize>;
    fn init(&self) -> Self::State {
        (None, MakeTxSubState::Init)
    }
}

impl InterpParser<MakeTransferTxParameters> for MakeTx {
    fn parse<'a, 'b>(&self, (ref mut hasherAndPath, ref mut state): &'b mut Self::State, chunk: &'a [u8], destination: &mut Option<Self::Returning>) -> ParseResult<'a> {
        let mut cursor = chunk;
        loop {
            match state {
                MakeTxSubState::Init => {
                    info!("State sizes2 \nMakeTx: {}\n", core::mem::size_of::<MakeTxSubState>());
                    init_with_default(destination);
                    set_from_thunk(state, || MakeTxSubState::PathRecipientAmount(<PathRecipientAmountT as ParserCommon<MakeTransferTxParameters1>>::init(&PathRecipientAmountP)))
                }
                MakeTxSubState::PathRecipientAmount(ref mut sub) => {
                    cursor = <PathRecipientAmountT as InterpParser<MakeTransferTxParameters1>>::parse(&PathRecipientAmountP, sub, cursor, hasherAndPath)?;
                    set_from_thunk(state, || MakeTxSubState::MetaNonce(<MetaNonceT as ParserCommon<MakeTransferTxParameters2>>::init(&MetaNonceP)))
                }
                MakeTxSubState::MetaNonce(ref mut sub) => {
                    cursor = <MetaNonceT as InterpParser<MakeTransferTxParameters2>>::parse(&MetaNonceP, sub, cursor, hasherAndPath)?;
                    set_from_thunk(state, || MakeTxSubState::Done);
                }
                MakeTxSubState::Done => {
                    match hasherAndPath {
                        Some((ref mut hasher, ref mut privkey)) => {
                            let mut f = || -> Option<()> {
                                final_accept_prompt(&[&"Sign Transaction?"])?;
                                *destination=Some(ArrayVec::new());
                                {
                                    let hash = hasher.finalize();
                                    let sig = eddsa_sign(&hash.0, &privkey)?;
                                    destination.as_mut()?.try_extend_from_slice(&sig.0[..]).ok()?;
                                }

                                {
                                    let pubkey = get_pubkey_from_privkey(privkey).ok()?;
                                    destination.as_mut()?.try_extend_from_slice(&pubkey.W[1..pubkey.W_len as usize]).ok()?;
                                }
                                Some(())
                            };
                            break f().map_or(Err((Some(OOB::Reject), cursor)), |_| Ok(cursor))
                        }
                        _ => {
                            // panic!("should have been set")
                        }
                    }
                }
            }
        }
    }
}

pub static MAKE_TRANSFER_TX_IMPL: MakeTransferTxImplT = MakeTx;
// pub static MAKE_TRANSFER_TX_IMPL: MakeTransferTxImplT = MoveAction(
//     PathRecipientAmountP
//     ,
//     mkmvfn(|(hash, path):HasherAndPath2, destination: &mut _| {
//         final_accept_prompt(&[&"Sign Transaction Hash?"])?;

//         // let key = get_private_key(&path).ok()?;
//         // let sig = eddsa_sign(&hash, &key)?;
//         // let mut rv = ArrayVec::<u8, 128>::new();
//         // rv.try_extend_from_slice(&sig.0[..]).ok()?;
//         // *destination = Some(rv);
//         Some(())
//     }),
// );

// pub static MAKE_TRANSFER_TX_IMPL_OLD: MakeTransferTxImplT = MoveAction(
//     DynBind(PathRecipientAmountP, MetaNonceP)
//     ,
//     mkmvfn(|(hash, path): ([u8; 32], ArrayVec<u32, 10>), destination: &mut _| {
//         final_accept_prompt(&[&"Sign Transaction Hash?"])?;

//         // let key = get_private_key(&path).ok()?;
//         // let sig = eddsa_sign(&hash, &key)?;
//         // let mut rv = ArrayVec::<u8, 128>::new();
//         // rv.try_extend_from_slice(&sig.0[..]).ok()?;
//         // *destination = Some(rv);
//         Some(())
//     }),
// );

// The global parser state enum; any parser above that'll be used as the implementation for an APDU
// must have a field here.

pub enum ParsersState {
    NoState,
    SettingsState(u8),
    GetAddressState(<GetAddressImplT as ParserCommon<Bip32Key>>::State),
    SignState(<SignImplT as ParserCommon<SignParameters>>::State),
    SignHashState(<SignHashImplT as ParserCommon<SignHashParameters>>::State),
    MakeTransferTxState(<MakeTransferTxImplT as ParserCommon<MakeTransferTxParameters>>::State),
}

pub fn reset_parsers_state(state: &mut ParsersState) {
    *state = ParsersState::NoState;
}

meta_definition!{}
kadena_capability_definition!{}
signer_definition!{}
payload_definition!{}
command_definition!{}
kadena_cmd_definition!{}

#[inline(never)]
pub fn get_get_address_state(
    s: &mut ParsersState,
) -> &mut <GetAddressImplT as ParserCommon<Bip32Key>>::State {
    match s {
        ParsersState::GetAddressState(_) => {}
        _ => {
            info!("Non-same state found; initializing state.");
            *s = ParsersState::GetAddressState(<GetAddressImplT as ParserCommon<Bip32Key>>::init(
                &GET_ADDRESS_IMPL,
            ));
        }
    }
    match s {
        ParsersState::GetAddressState(ref mut a) => a,
        _ => {
            panic!("")
        }
    }
}

#[inline(never)]
pub fn get_sign_state(
    s: &mut ParsersState,
) -> &mut <SignImplT as ParserCommon<SignParameters>>::State {
    match s {
        ParsersState::SignState(_) => {}
        _ => {
            info!("Non-same state found; initializing state.");
            *s = ParsersState::SignState(<SignImplT as ParserCommon<SignParameters>>::init(
                &SIGN_IMPL,
            ));
        }
    }
    match s {
        ParsersState::SignState(ref mut a) => a,
        _ => {
            panic!("")
        }
    }
}

#[inline(never)]
pub fn get_sign_hash_state(
    s: &mut ParsersState,
) -> &mut <SignHashImplT as ParserCommon<SignHashParameters>>::State {
    match s {
        ParsersState::SignHashState(_) => {}
        _ => {
            info!("Non-same state found; initializing state.");
            *s = ParsersState::SignHashState(<SignHashImplT as ParserCommon<SignHashParameters>>::init(
                &SIGN_HASH_IMPL,
            ));
        }
    }
    match s {
        ParsersState::SignHashState(ref mut a) => a,
        _ => {
            panic!("")
        }
    }
}

#[inline(never)]
pub fn get_make_transfer_tx_state(
    s: &mut ParsersState,
) -> &mut <MakeTransferTxImplT as ParserCommon<MakeTransferTxParameters>>::State {
    match s {
        ParsersState::MakeTransferTxState(_) => {}
        _ => {
            info!("Non-same state found; initializing state.");
            *s = ParsersState::MakeTransferTxState(<MakeTransferTxImplT as ParserCommon<MakeTransferTxParameters>>::init(
                &MAKE_TRANSFER_TX_IMPL,
            ));
        }
    }
    match s {
        ParsersState::MakeTransferTxState(ref mut a) => a,
        _ => {
            panic!("")
        }
    }
}
