import assert from 'node:assert/strict';
import {capture,pack,unpack} from './packed.mjs';
const scalar=kind=>({kind}),list=element=>({kind:'list',element});
for(const kind of ['f32','f64']) {
 const type=list(scalar(kind)),tag=kind==='f32'?'F32Bits':'F64Bits';
 const bits=kind==='f32'?[0,0x80000000,0x3f800000,0x7f800000,0xff800000,0x7fc12345,0xff812345]:['0','9223372036854775808','4607182418800017408','9218868437227405312','18442240474082181120','9221120237041095220','18444492273895866369'];
 const values=[0,-0,1/3,...bits.map(b=>({[tag]:b}))];
 const before=pack(values,type),owned=capture(values,type);
 assert.deepEqual(pack(owned.javascript,type).words,before.words);
 assert.deepEqual(owned.javascript,unpack(before.words,type,true));
 assert.equal(owned.finite,false);assert.equal(Object.hasOwn(owned,'words'),false);
 values[0]=9;values.at(-1)[tag]=kind==='f32'?0:'0';
 assert.deepEqual(pack(owned.javascript,type).words,before.words);
 for(const bad of [Infinity,-Infinity,NaN,{}, {[tag]:-1}, {[tag]:kind==='f32'?2**32:'18446744073709551616'}])assert.throws(()=>capture([bad],type));
}
// No bit-array allocation or DataView conversion for ordinary finite floats.
const OldView=globalThis.DataView,OldWords=globalThis.Uint32Array;
try {
 globalThis.DataView=class {constructor(){throw Error('float capture converted bits');}};
 globalThis.Uint32Array=class extends OldWords {constructor(){throw Error('float capture packed words');}};
 assert.deepEqual(capture([1/3,-0],list(scalar('f32'))).javascript,[Math.fround(1/3),-0]);
}finally{globalThis.DataView=OldView;globalThis.Uint32Array=OldWords;}
const words=new Uint32Array([1,4294967295]);words[Symbol.iterator]=()=>{throw Error('iterator')};words.slice=()=>{throw Error('slice')};
const proxy=new Proxy(words,{get:(t,k)=>Reflect.get(t,k,t),ownKeys(){throw Error('enumeration')}});
const owned=capture(proxy,list(scalar('u32')));words[0]=99;
assert.deepEqual([...owned.javascript],[1,4294967295]);assert.strictEqual(owned.words,owned.javascript);
const order=[],record={kind:'record',fields:[{name:'b',type:scalar('f32')},{name:'a',type:scalar('u64')}]};
const input={get a(){order.push('a');return '9007199254740993'},get b(){order.push('b');return -0}};
const snapshot=capture(input,record);assert.deepEqual(order,['b','a']);assert.equal(snapshot.javascript.a,9007199254740993n);assert(Object.is(snapshot.javascript.b,-0));
const proto={kind:'record',fields:[{name:'__proto__',type:scalar('u32')}]};
assert.deepEqual(capture(JSON.parse('{"__proto__":7}'),proto).javascript,JSON.parse('{"__proto__":7}'));
const mutable=Object.freeze({kind:'list',element:{kind:'u32'}});capture([1],mutable);mutable.element.kind='bool';assert.deepEqual(capture([true],mutable).javascript,[true]);
let changingKind='u32';const accessor=Object.freeze({get kind(){return changingKind;}});
assert.equal(capture(1,accessor).javascript,1);changingKind='bool';assert.equal(capture(true,accessor).javascript,true);
let changingElement={kind:'u32'};const nestedAccessor=Object.freeze({kind:'list',get element(){return changingElement;}});
assert.deepEqual(capture([1],nestedAccessor).javascript,new Uint32Array([1]));changingElement={kind:'bool'};assert.deepEqual(capture([true],nestedAccessor).javascript,[true]);
for(const t of [scalar('u32'),scalar('i32')])assert(Object.is(capture(-0,t).javascript,0));
assert.throws(()=>capture({length:67108865},list(scalar('f32'))),/budget/);
assert.throws(()=>capture(new DataView(new ArrayBuffer(8)),list(scalar('f32'))));
assert.deepEqual(capture([,1],list(scalar('u32'))).javascript,new Uint32Array([0,1]));
