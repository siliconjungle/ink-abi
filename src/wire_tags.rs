/// Literal tags for the handle-based value ABI. Shared by emitters and support.
#[repr(u32)]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum InkValueKind {Unit=0,Bool=1,U32=2,U64=3,I32=4,F32=5,Int=6,String=7,Enum=8}
impl InkValueKind {
    pub fn from_raw(n:u32)->Option<Self>{Some(match n{0=>Self::Unit,1=>Self::Bool,2=>Self::U32,3=>Self::U64,4=>Self::I32,5=>Self::F32,6=>Self::Int,7=>Self::String,8=>Self::Enum,_=>return None})}
}
