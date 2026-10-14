import assert from 'node:assert/strict';
import {createValueCodec,createFusedValueCodec} from './value-wire.mjs';
const scalar=kind=>({kind}),mode={kind:'enum',name:'Mode',variants:['On','Off']},id={kind:'id',name:'Key'};
const node={kind:'record',name:'Node',fields:[{name:'text',type:scalar('string')},{name:'next',type:{kind:'option',element:{kind:'ref',name:'Node'}}}]};
const root={kind:'record',name:'Root',fields:[
 {name:'node',type:node},{name:'counts',type:{kind:'vector',element:scalar('u32'),size:2}},
 {name:'mode',type:mode},{name:'id',type:id},{name:'big',type:scalar('int')},
 {name:'signed',type:scalar('i32')},{name:'float',type:scalar('f32')},{name:'flag',type:scalar('bool')},
 {name:'unit',type:scalar('unit')},{name:'reply',type:{kind:'result',ok:scalar('u64'),error:scalar('string')}}]};
const framed=body=>reader=>{const end=reader.frame(),value=body(reader,end);reader.finish(end);return value;};
const text=framed((reader,end)=>reader.text(end));
let readNode;
const option=framed(reader=>reader.byte()?{Some:readNode(reader)}:{None:null});
readNode=framed(reader=>({text:text(reader),next:option(reader)}));
const word=framed(reader=>reader.word());
const vector=framed(reader=>{const n=reader.count(2),values=new Array(n);for(let i=0;i<n;i++)values[i]=word(reader);return values;});
const readMode=framed((reader,end)=>reader.text(end,'enum',['On','Off']));
const readId=framed((reader,end)=>reader.text(end,'id'));
const integer=framed((reader,end)=>({Int:reader.text(end,'int')}));
const signed=framed(reader=>reader.signed()),float=framed(reader=>({F32Bits:reader.word()}));
const flag=framed(reader=>!!reader.byte()),unit=framed(()=>null),wide=framed(reader=>reader.wide());
const result=framed(reader=>reader.byte()?{Err:text(reader)}:{Ok:wide(reader)});
const read=framed(reader=>({node:readNode(reader),counts:vector(reader),mode:readMode(reader),id:readId(reader),big:integer(reader),signed:signed(reader),float:float(reader),flag:flag(reader),unit:unit(reader),reply:result(reader)}));
const oracle=await createValueCodec(root,{Node:node}),fused=createFusedValueCodec(oracle.descriptor,read);
const samples=[
 {node:{text:'雪😀',next:{Some:{text:'\ufeff',next:{None:null}}}},counts:[0,4294967295],mode:'On',id:'0'.repeat(32),big:{Int:'-123456789012345678901234567890'},signed:-2147483648,float:{F32Bits:0x7fc12345},flag:true,unit:null,reply:{Ok:18446744073709551615n}},
 {node:{text:'',next:{None:null}},counts:[1,2],mode:'Off',id:'abcdef0123456789abcdef0123456789',big:{Int:'0'},signed:2147483647,float:{F32Bits:0x80000000},flag:false,unit:null,reply:{Err:'error'}},
];
const outcome=run=>{try{return {ok:true,value:run()};}catch{return {ok:false};}};
let checks=0;
for(const compact of [false,true]){
 const baseline=compact?oracle.compact():oracle,selected=compact?fused.compact():fused;
 for(const sample of samples){
  const bytes=baseline.encode(sample);
  const compare=(input,limits={})=>{
   assert.deepEqual(outcome(()=>selected.decode(input,limits)),outcome(()=>baseline.decode(input,limits)));
   checks++;
  };
  compare(bytes);const owned=selected.decode(bytes);bytes.fill(0);assert.deepEqual(owned,sample);
  const original=baseline.encode(sample);
  for(const limits of [{nodes:0},{nodes:1},{nodes:3},{nodes:20},{nodes:100},{depth:0},{depth:1},{depth:2},{depth:3},{bytes:0},{bytes:40},{bytes:original.length-1},{bytes:original.length}])compare(original,limits);
  for(let i=0;i<original.length;i++){const corrupt=original.slice();corrupt[i]^=255;compare(corrupt);}
  for(let end=0;end<original.length;end++)compare(original.subarray(0,end));
  const extra=new Uint8Array(original.length+1);extra.set(original);compare(extra);
  if(typeof SharedArrayBuffer==='function'){const shared=new Uint8Array(new SharedArrayBuffer(original.length));shared.set(original);compare(shared);}
 }
}
// Canonical compact counts must fail identically, even when the decoded size fits.
const strings=await createValueCodec(scalar('string')),fusedStrings=createFusedValueCodec(strings.descriptor,text);
for(const bytes of [Uint8Array.of(1,129,0,65),Uint8Array.of(1,128,128,128,128,16),Uint8Array.of(1,1,255)]){
 assert.deepEqual(outcome(()=>fusedStrings.compact().decode(bytes)),outcome(()=>strings.compact().decode(bytes)));
}
console.log(checks,'fused-reader outcomes agree with independent decoding, corruptions, truncations, limits and detached outputs');
