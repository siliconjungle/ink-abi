// Shared packed value transport. Executed by every browser host.
const MAX_BYTES=256*1024*1024;
const fail=s=>{throw Error(s)};
export function width(t,depth=0) {
  if(depth>32||!t||typeof t!=='object')fail('invalid packed type');
  if(t.kind==='list') {if(depth!==0)fail('nested packed list');return width(t.element,depth+1);}
  if(t.kind==='vector') {if(!Number.isSafeInteger(t.size)||t.size<1)fail('invalid vector size');const n=t.size*width(t.element,depth+1);if(!Number.isSafeInteger(n))fail('packed width overflow');return n;}
  if(t.kind==='record') {if(!Array.isArray(t.fields)||!t.fields.length||new Set(t.fields.map(f=>f.name)).size!==t.fields.length||t.fields.some(f=>typeof f.name!=='string'))fail('invalid packed record');const n=t.fields.reduce((n,f)=>n+width(f.type,depth+1),0);if(!Number.isSafeInteger(n))fail('packed width overflow');return n;}
  if(t.kind==='u64'||t.kind==='f64')return 2;
  if(['u32','i32','f32','bool'].includes(t.kind))return 1;
  fail('unsupported packed type');
}

// Snapshot the same packed input domain into its native JS representation.
// Transport words are materialized only after a CPU/device choice. Fixed type
// copiers may be retained only for immutable descriptors, never mutable callers.
const captures=new WeakMap();
export function capture(v,t) {
  let copier=Object.isFrozen(t)?captures.get(t):null;
  if(!copier){copier=compileCapture(t);if(immutableType(t))captures.set(t,copier);}
  return copier(v);
}
function fixedData(value,keys){return Object.isFrozen(value)&&keys.every(key=>Object.hasOwn(Object.getOwnPropertyDescriptor(value,key)??{},'value'));}
function immutableType(t){
  if(!fixedData(t,['kind']))return false;
  if(t.kind==='record')return fixedData(t,['fields'])&&fixedData(t.fields,Array.from({length:t.fields.length},(_,i)=>String(i)))&&t.fields.every(f=>fixedData(f,['name','type'])&&immutableType(f.type));
  if(t.kind==='list'||t.kind==='vector')return fixedData(t,t.kind==='vector'?['element','size']:['element'])&&immutableType(t.element);
  return true;
}
function compileCapture(type) {
  const stride=width(type);
  function compile(t) {
    if(t.kind==='list'||t.kind==='vector') {
      const element=compile(t.element),list=t.kind==='list';
      return (v,state)=>{
        if((!Array.isArray(v)&&!ArrayBuffer.isView(v)&&!(list&&v instanceof Uint32Array))||(!list&&v.length!==t.size))fail(list?'expected list':'vector dimension mismatch');
        if(list&&t.element.kind==='u32') {
          // set consumes array-like typed captures without invoking a caller's
          // iterator or overridable slice. The one owned copy is also transport.
          if(v instanceof Uint32Array){const out=new Uint32Array(v.length);out.set(v);return out;}
          if(v.some(x=>!Number.isInteger(x)||x<0||x>4294967295))fail('u32 outside range');
          return Uint32Array.from(v);
        }
        const out=[];for(const x of v)out.push(element(x,state));return out;
      };
    }
    if(t.kind==='record') {
      const fields=t.fields.map(f=>[f.name,compile(f.type)]);
      return (v,state)=>{
        if(!v||typeof v!=='object'||Array.isArray(v)||Object.keys(v).length!==fields.length||fields.some(([name])=>!Object.hasOwn(v,name)))fail('record fields mismatch');
        const out={};
        for(const [name,copy] of fields){const value=copy(v[name],state);if(name==='__proto__')Object.defineProperty(out,name,{value,enumerable:true,writable:true,configurable:true});else out[name]=value;}
        return out;
      };
    }
    switch(t.kind) {
      case 'bool':return v=>{if(typeof v!=='boolean')fail('expected Bool');return v;};
      case 'u32':case 'i32':return v=>{if(!Number.isInteger(v)||v<(t.kind==='u32'?0:-2147483648)||v>(t.kind==='u32'?4294967295:2147483647))fail('integer outside range');return v===0?0:v;};
      case 'u64':return v=>{if(typeof v==='number'&&!Number.isSafeInteger(v))fail('u64 needs an exact integer or decimal string');if(!['number','string','bigint'].includes(typeof v))fail('expected u64');const n=BigInt(v);if(n<0n||n>18446744073709551615n)fail('u64 outside range');return n;};
      case 'f32':return (v,state)=>{
        if(typeof v==='number'){const n=Math.fround(v);if(!Number.isFinite(n))fail('f32 input overflow');return n;}
        if(!v||typeof v!=='object'||Object.keys(v).length!==1)fail('invalid F32Bits');
        const bits=v.F32Bits;if(!Number.isInteger(bits)||bits<0||bits>4294967295)fail('invalid F32Bits');
        const view=state.view??=(new DataView(new ArrayBuffer(8)));view.setUint32(0,bits,true);const n=view.getFloat32(0,true);
        if(Number.isFinite(n))return n;state.finite=false;return {F32Bits:bits};
      };
      case 'f64':return (v,state)=>{
        if(typeof v==='number'&&Number.isFinite(v))return v;
        if(!v||typeof v!=='object'||Object.keys(v).length!==1||!Object.hasOwn(v,'F64Bits'))fail('invalid F64Bits');
        const text=v.F64Bits;if(typeof text!=='string'||text.length>20||!/^(0|[1-9][0-9]*)$/.test(text))fail('invalid F64Bits');
        const bits=BigInt(text);if(bits>18446744073709551615n)fail('invalid F64Bits');
        const view=state.view??=(new DataView(new ArrayBuffer(8)));view.setBigUint64(0,bits,true);const n=view.getFloat64(0,true);
        if(Number.isFinite(n))return n;state.finite=false;return {F64Bits:text};
      };
      default:fail('unsupported packed type');
    }
  }
  const copy=compile(type);
  return v=>{
    const count=type.kind==='list'?v?.length:1,byteLength=count*stride*4;
    if(byteLength>MAX_BYTES)fail('input exceeds 256 MiB budget');
    const state={finite:true},javascript=copy(v,state);
    if(type.kind==='list'&&javascript.length*stride*4>MAX_BYTES)fail('input exceeds 256 MiB budget');
    return {javascript,type,count,finite:state.finite,byteLength,...(type.kind==='list'&&type.element.kind==='u32'&&javascript instanceof Uint32Array?{words:javascript}:{})};
  };
}

export function pack(v, t) {
  const stride=width(t);
  if(t.kind==='list'&&(!Array.isArray(v)&&!ArrayBuffer.isView(v)&&!(v instanceof Uint32Array)))fail('expected list');
  if((t.kind==='list'?v.length:1)*stride*4>MAX_BYTES)fail('input exceeds 256 MiB budget');
  if(t.kind === 'list' && t.element.kind === 'u32') {
    if(v.length * 4 > MAX_BYTES) fail('input exceeds 256 MiB budget');
    if(!(v instanceof Uint32Array)&&v.some(x=>!Number.isInteger(x)||x<0||x>4294967295))fail('u32 outside range');
    return {words:v instanceof Uint32Array ? v.slice() : Uint32Array.from(v),type:t,count:v.length,finite:true};
  }
  const words = [],
    view = new DataView(new ArrayBuffer(8));
  let finite = true;
  function go(v, t) {
    switch (t.kind) {
      case "list":
        for (const x of v) go(x, t.element);
        break;
      case "vector":
        if((!Array.isArray(v)&&!ArrayBuffer.isView(v))||v.length!==t.size)fail("vector dimension mismatch");
        for (const x of v) go(x, t.element);
        break;
      case "record":
        if(!v||typeof v!=="object"||Array.isArray(v)||Object.keys(v).length!==t.fields.length||t.fields.some(f=>!Object.hasOwn(v,f.name)))fail("record fields mismatch");
        for (const f of t.fields) {
          go(v[f.name], f.type);
        }
        break;
      case "bool":
        if(typeof v!=="boolean")fail("expected Bool");
        words.push(+v);
        break;
      case "u32":
      case "i32":
        if(!Number.isInteger(v)||v<(t.kind==="u32"?0:-2147483648)||v>(t.kind==="u32"?4294967295:2147483647))fail("integer outside range");
        words.push(v >>> 0);
        break;
      case "u64": {
        if(typeof v==="number"&&!Number.isSafeInteger(v))fail("u64 needs an exact integer or decimal string");
        if(!["number","string","bigint"].includes(typeof v))fail("expected u64");
        const n = BigInt(v);if(n<0n||n>18446744073709551615n)fail("u64 outside range");
        words.push(Number(n & 4294967295n), Number(n >> 32n));
        break;
      }
      case "f64": {
        let bits;
        if(typeof v==='number'&&Number.isFinite(v)){view.setFloat64(0,v,true);bits=view.getBigUint64(0,true);}
        else{
          if(!v||typeof v!=='object'||Object.keys(v).length!==1||!Object.hasOwn(v,'F64Bits'))fail('invalid F64Bits');const text=v.F64Bits;
          if(typeof text!=='string'||text.length>20||!/^(0|[1-9][0-9]*)$/.test(text))fail('invalid F64Bits');
          bits=BigInt(text);if(bits>18446744073709551615n)fail('invalid F64Bits');
          view.setBigUint64(0,bits,true);finite &&= Number.isFinite(view.getFloat64(0,true));
        }
        words.push(Number(bits&4294967295n),Number(bits>>32n));break;
      }
      case "f32":
        if (typeof v === "number") {
          view.setFloat32(0, v, true);
          if(!Number.isFinite(view.getFloat32(0,true)))fail("f32 input overflow");
          words.push(view.getUint32(0, true));
        } else {
          if(!v||typeof v!=="object"||Object.keys(v).length!==1||!Number.isInteger(v.F32Bits)||v.F32Bits<0||v.F32Bits>4294967295)fail("invalid F32Bits");
          view.setUint32(0, v.F32Bits, true);
          finite &&= Number.isFinite(view.getFloat32(0, true));
          words.push(v.F32Bits);
        }
        break;
      default:
        fail("unsupported packed type");
    }
  }
  go(v, t);
  if (words.length * 4 > MAX_BYTES) fail("input exceeds 256 MiB budget");
  return {
    words: Uint32Array.from(words),
    type: t,
    count: t.kind === "list" ? v.length : 1,
    finite,
  };
}
export function unpack(words, t, native = false, destination = null) {
  const stride=width(t);if(words.length%stride||t.kind!=="list"&&words.length!==stride)fail("invalid result layout");
  let i = 0;
  const view = new DataView(new ArrayBuffer(8));
  function go(t) {
    switch (t.kind) {
      case "list": {
        const n = words.length / width(t.element);
        if (!Number.isInteger(n)) fail("invalid result layout");
        if(destination){if(destination.length<n)fail("destination capacity");for(let j=0;j<n;j++)destination[j]=go(t.element);return destination;}
        return Array.from({ length: n }, () => go(t.element));
      }
      case "vector":
        return Array.from({ length: t.size }, () => go(t.element));
      case "record":
        return Object.fromEntries(t.fields.map((f) => [f.name, go(f.type)]));
      case "bool":
        return words[i++] !== 0;
      case "u32":
        return words[i++];
      case "i32":
        return words[i++] | 0;
      case "u64": {
        const n = BigInt(words[i++]) + (BigInt(words[i++]) << 32n);
        return native ? n : n <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(n) : n.toString();
      }
      case "f64": {
        const bits=BigInt(words[i++])|(BigInt(words[i++])<<32n);
        view.setBigUint64(0,bits,true);const value=view.getFloat64(0,true);
        return Number.isFinite(value)?value:{F64Bits:bits.toString()};
      }
      case "f32":
        view.setUint32(0, words[i++], true);
        return Number.isFinite(view.getFloat32(0, true))
          ? view.getFloat32(0, true)
          : { F32Bits: view.getUint32(0, true) };
      default:
        fail("invalid result type");
    }
  }
  return go(t);
}
