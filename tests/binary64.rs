use ink_abi::{float64,packed,typed_json::TypedJsonCodec,value_wire::{Limits,Schema}};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn exact_binary64_wire_packed_and_javascript_boundaries(){
 let scalar=json!({"kind":"f64"});let schema=Schema::new(scalar.clone(),BTreeMap::new()).unwrap();let limits=Limits::default();
 assert_eq!(ink_abi::describe(&"F64",&json!({}),&json!({}),&json!([])).unwrap(),scalar);
 assert_eq!(ink_abi::packed_width(&scalar).unwrap(),2);
 let mut patterns=vec![0,1<<63,1,0x000fffffffffffff,0x0010000000000000,0x3ff0000000000001,0x7fefffffffffffff,0x7ff0000000000000,0xfff0000000000000,0x7ff0000000001234,0xfff8000000005678,u64::MAX];
 let mut random=0x8a9375a43b7e19cdu64;
 for _ in 0..256{random^=random<<13;random^=random>>7;random^=random<<17;patterns.push(random);}
 let mut cases=vec![];
 for bits in patterns{
  let value=float64::raw(bits);let bytes=schema.encode(&value,limits).unwrap();
  assert_eq!(&bytes[40..44],&8u32.to_le_bytes());assert_eq!(&bytes[44..],&bits.to_le_bytes());
  assert_eq!(schema.decode(&bytes,limits).unwrap(),value);
  let d=packed::pack(&value,&scalar).unwrap();assert_eq!(d.words.as_slice(),[bits as u32,(bits>>32)as u32]);
  assert_eq!(d.finite,f64::from_bits(bits).is_finite());
  assert_eq!(float64::bits(&packed::unpack(&d,&scalar)).unwrap(),bits);
  let bridge=ink_abi::wire_packed::pack_flat(schema.validate(&bytes,limits).unwrap()).unwrap();
  assert_eq!(bridge.words,d.words);assert_eq!(bridge.finite,d.finite);
  assert!(schema.validate(&bytes[..bytes.len()-1],limits).is_err());
  assert!(schema.validate(&bytes,Limits{nodes:0,..limits}).is_err());
  cases.push(json!({"value":value,"bytes":bytes,"words":d.words.as_ref(),"finite":d.finite}));
 }
 let invalid=vec![json!({"F64Bits":1}),json!({"F64Bits":""}),json!({"F64Bits":"00"}),json!({"F64Bits":"-0"}),json!({"F64Bits":"+1"}),json!({"F64Bits":"18446744073709551616"}),json!({"F64Bits":" 1"}),json!({"F64Bits":"1","extra":0}),json!({"F32Bits":0})];
 for value in &invalid{assert!(schema.encode(value,limits).is_err());assert!(packed::pack(value,&scalar).is_err());}
 for (tag,kind)in [(0,ink_abi::ValueKind::Unit),(5,ink_abi::ValueKind::F32),(8,ink_abi::ValueKind::Enum),(9,ink_abi::ValueKind::F64)]{assert_eq!(ink_abi::ValueKind::from_raw(tag),Some(kind));}
 assert_eq!(ink_abi::ValueKind::from_raw(10),None);
 let dir=std::env::temp_dir().join(format!("ink-binary64-abi-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
 for(name,source)in [("value-wire.mjs",ink_abi::VALUE_WIRE_JS_SOURCE),("packed.mjs",ink_abi::PACKED_JS_SOURCE),("json-value.mjs",ink_abi::JSON_VALUE_JS_SOURCE),("run.mjs",include_str!("fixtures/binary64.mjs"))]{std::fs::write(dir.join(name),source).unwrap();}
 std::fs::write(dir.join("cases.json"),serde_json::to_vec(&json!({"descriptor":schema.descriptor(),"cases":cases,"invalid":invalid})).unwrap()).unwrap();
 let run=std::process::Command::new("node").arg(dir.join("run.mjs")).output().unwrap();std::fs::remove_dir_all(dir).unwrap();
 assert!(run.status.success(),"{}",String::from_utf8_lossy(&run.stderr));
}

#[test]
fn typed_json_roundtrips_binary64_precision_and_rejects_nonfinite_numbers(){
 let schema=Schema::new(json!({"kind":"f64"}),BTreeMap::new()).unwrap();let codec=TypedJsonCodec::new(schema.clone());let l=Limits::default();
 for (text,bits)in [("-0",1<<63),("1.0000000000000002",0x3ff0000000000001),("5e-324",1),("1.7976931348623157e308",0x7fefffffffffffff)]{
  let value=float64::raw(bits);assert_eq!(codec.parse(text,l).unwrap(),value);
  let wire=codec.parse_binary(text,l).unwrap();assert_eq!(schema.decode(&wire,l).unwrap(),value);
  assert_eq!(codec.parse(&codec.stringify_binary(&wire,l).unwrap(),l).unwrap(),value);
  assert_eq!(codec.parse(&codec.stringify(&value,l).unwrap(),l).unwrap(),value);
 }
 for text in ["1e309","NaN","Infinity","null","{\"F64Bits\":\"0\"}"]{assert!(codec.parse(text,l).is_err());assert!(codec.parse_binary(text,l).is_err());}
 for bits in [0x7ff0000000000000,0x7ff0000000001234]{let value=float64::raw(bits);let wire=schema.encode(&value,l).unwrap();assert!(codec.stringify(&value,l).is_err());assert!(codec.stringify_binary(&wire,l).is_err());}
}
