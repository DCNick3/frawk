use super::llvm::{
    self,
    prelude::{LLVMContextRef, LLVMModuleRef, LLVMTypeRef, LLVMValueRef},
};
use crate::builtins::Variable;
use crate::common::Either;
use crate::libc::c_void;
use crate::runtime::{
    self, FileRead, FileWrite, Float, Int, IntMap, LazyVec, RegexCache, Str, StrMap, Variables,
};
use hashbrown::HashMap;

use std::cell::RefCell;
use std::convert::TryFrom;
use std::mem;

macro_rules! fail {
    ($($es:expr),+) => {{
        #[cfg(test)]
        {
            panic!("failure in runtime {}. Halting execution", format!($($es),*))
        }
        #[cfg(not(test))]
        {
            eprintln!("failure in runtime {}. Halting execution", format!($($es),*));
            std::process::abort()
        }
    }}
}

pub(crate) struct Runtime<'a> {
    vars: Variables<'a>,
    line: Str<'a>,
    split_line: LazyVec<Str<'a>>,
    regexes: RegexCache,
    write_files: FileWrite,
    read_files: FileRead,
}
impl<'a> Runtime<'a> {
    pub(crate) fn new(
        stdin: impl std::io::Read + 'static,
        stdout: impl std::io::Write + 'static,
    ) -> Runtime<'a> {
        Runtime {
            vars: Default::default(),
            line: "".into(),
            split_line: LazyVec::new(),
            regexes: Default::default(),
            write_files: FileWrite::new(stdout),
            read_files: FileRead::new(stdin),
        }
    }
}

struct Intrinsic {
    name: *const libc::c_char,
    data: RefCell<Either<LLVMTypeRef, LLVMValueRef>>,
    _func: *mut c_void,
}

// A map of intrinsics that lazily declares them when they are used in codegen.
pub(crate) struct IntrinsicMap {
    module: LLVMModuleRef,
    map: HashMap<&'static str, Intrinsic>,
}

impl IntrinsicMap {
    fn new(module: LLVMModuleRef) -> IntrinsicMap {
        IntrinsicMap {
            module,
            map: Default::default(),
        }
    }
    fn register(
        &mut self,
        name: &'static str,
        cname: *const libc::c_char,
        ty: LLVMTypeRef,
        _func: *mut c_void,
    ) {
        assert!(self
            .map
            .insert(
                name,
                Intrinsic {
                    name: cname,
                    data: RefCell::new(Either::Left(ty)),
                    _func,
                }
            )
            .is_none())
    }

    pub(crate) unsafe fn get(&self, name: &'static str) -> LLVMValueRef {
        use llvm::core::*;
        let intr = &self.map[name];
        let mut val = intr.data.borrow_mut();

        let ty = match &mut *val {
            Either::Left(ty) => *ty,
            Either::Right(v) => return *v,
        };
        let func = LLVMAddFunction(self.module, intr.name, ty);
        LLVMSetLinkage(func, llvm::LLVMLinkage::LLVMExternalLinkage);
        *val = Either::Right(func);
        func
    }
}

pub(crate) unsafe fn register(module: LLVMModuleRef, ctx: LLVMContextRef) -> IntrinsicMap {
    use llvm::core::*;
    let usize_ty = LLVMIntTypeInContext(ctx, (mem::size_of::<usize>() * 8) as libc::c_uint);
    let int_ty = LLVMIntTypeInContext(ctx, (mem::size_of::<Int>() * 8) as libc::c_uint);
    let float_ty = LLVMDoubleType();
    let void_ty = LLVMVoidType();
    let str_ty = LLVMIntTypeInContext(ctx, (mem::size_of::<Str>() * 8) as libc::c_uint);
    let rt_ty = LLVMPointerType(void_ty, 0);
    let str_ref_ty = LLVMPointerType(str_ty, 0);
    let mut table = IntrinsicMap::new(module);
    macro_rules! register_inner {
        ($name:ident, [ $($param:expr),* ], $ret:expr) => { {
            // Try and make sure the linker doesn't strip the function out.
            let mut params = [$($param),*];
            let ty = LLVMFunctionType($ret, params.as_mut_ptr(), params.len() as u32, 0);
            table.register(stringify!($name), c_str!(stringify!($name)), ty, $name as *mut c_void);
        }};
    }
    macro_rules! register {
        ($name:ident ($($param:expr),*); $($rest:tt)*) => {
            register_inner!($name, [ $($param),* ], void_ty);
            register!($($rest)*);
        };
        ($name:ident ($($param:expr),*) -> $ret:expr; $($rest:tt)*) => {
            register_inner!($name, [ $($param),* ], $ret);
            register!($($rest)*);
        };
        () => {};
    }

    register! {
        ref_str(str_ref_ty);
        drop_str(str_ref_ty);
        ref_map(usize_ty);
        drop_map(usize_ty);
        int_to_str(int_ty) -> str_ty;
        float_to_str(float_ty) -> str_ty;
        str_to_int(str_ref_ty) -> int_ty;
        str_to_float(str_ref_ty) -> float_ty;
        str_len(str_ref_ty) -> usize_ty;
        concat(str_ref_ty, str_ref_ty) -> str_ty;
        match_pat(rt_ty, str_ref_ty, str_ref_ty) -> int_ty;
        get_col(rt_ty, int_ty) -> str_ty;
        set_col(rt_ty, int_ty, str_ref_ty);
        split_int(rt_ty, str_ref_ty, usize_ty, str_ref_ty) -> int_ty;
        split_str(rt_ty, str_ref_ty, usize_ty, str_ref_ty) -> int_ty;
        print_stdout(rt_ty, str_ref_ty);
        print(rt_ty, str_ref_ty, str_ref_ty, int_ty);
        read_err(rt_ty, str_ref_ty) -> int_ty;
        read_err_stdin(rt_ty) -> int_ty;
        next_line(rt_ty, str_ref_ty) -> str_ty;
        next_line_stdin(rt_ty) -> str_ty;

        load_var_str(rt_ty, usize_ty) -> str_ty;
        store_var_str(rt_ty, usize_ty, str_ref_ty);
        load_var_int(rt_ty, usize_ty) -> int_ty;
        store_var_int(rt_ty, usize_ty, int_ty);
        load_var_intmap(rt_ty, usize_ty) -> usize_ty;
        store_var_intmap(rt_ty, usize_ty, usize_ty);

        str_lt(str_ref_ty, str_ref_ty) -> int_ty;
        str_gt(str_ref_ty, str_ref_ty) -> int_ty;
        str_lte(str_ref_ty, str_ref_ty) -> int_ty;
        str_gte(str_ref_ty, str_ref_ty) -> int_ty;
        str_eq(str_ref_ty, str_ref_ty) -> int_ty;

        alloc_intint() -> usize_ty;
        len_intint(usize_ty) -> int_ty;
        lookup_intint(usize_ty, int_ty) -> int_ty;
        contains_intint(usize_ty, int_ty) -> int_ty;
        insert_intint(usize_ty, int_ty, int_ty);
        delete_intint(usize_ty, int_ty);

        alloc_intfloat() -> usize_ty;
        len_intfloat(usize_ty) -> int_ty;
        lookup_intfloat(usize_ty, int_ty) -> float_ty;
        contains_intfloat(usize_ty, int_ty) -> int_ty;
        insert_intfloat(usize_ty, int_ty, float_ty);
        delete_intfloat(usize_ty, int_ty);

        alloc_intstr() -> usize_ty;
        len_intstr(usize_ty) -> int_ty;
        lookup_intstr(usize_ty, int_ty) -> str_ty;
        contains_intstr(usize_ty, int_ty) -> int_ty;
        insert_intstr(usize_ty, int_ty, str_ref_ty);
        delete_intstr(usize_ty, int_ty);

        alloc_strint() -> usize_ty;
        len_strint(usize_ty) -> int_ty;
        lookup_strint(usize_ty, str_ref_ty) -> int_ty;
        contains_strint(usize_ty, str_ref_ty) -> int_ty;
        insert_strint(usize_ty, str_ref_ty, int_ty);
        delete_strint(usize_ty, str_ref_ty);

        alloc_strfloat() -> usize_ty;
        len_strfloat(usize_ty) -> int_ty;
        lookup_strfloat(usize_ty, str_ref_ty) -> float_ty;
        contains_strfloat(usize_ty, str_ref_ty) -> int_ty;
        insert_strfloat(usize_ty, str_ref_ty, float_ty);
        delete_strfloat(usize_ty, str_ref_ty);

        alloc_strstr() -> usize_ty;
        len_strstr(usize_ty) -> int_ty;
        lookup_strstr(usize_ty, str_ref_ty) -> str_ty;
        contains_strstr(usize_ty, str_ref_ty) -> int_ty;
        insert_strstr(usize_ty, str_ref_ty, str_ref_ty);
        delete_strstr(usize_ty, str_ref_ty);
    };
    table
}

// TODO: Iterators
// TODO: IO Errors.
//  - we need to exit cleanly. Add a "checkIOerror" builtin to main? set a variable in the runtime
//    and exit cleanly?
//  - get this working along with iterators after everything else is working.
//  - in gawk: redirecting to an output file that fails creates an error; but presumably we want to
//    handle stdout being closed gracefully.

#[no_mangle]
pub unsafe extern "C" fn read_err(runtime: *mut c_void, file: *mut c_void) -> Int {
    let runtime = &mut *(runtime as *mut Runtime);
    let res = match runtime.read_files.read_err(&*(file as *mut Str)) {
        Ok(res) => res,
        Err(e) => fail!("unexpected error when reading error status of file: {}", e),
    };
    res
}

#[no_mangle]
pub unsafe extern "C" fn read_err_stdin(runtime: *mut c_void) -> Int {
    let runtime = &mut *(runtime as *mut Runtime);
    runtime.read_files.read_err_stdin()
}

#[no_mangle]
pub unsafe extern "C" fn next_line_stdin(runtime: *mut c_void) -> u128 {
    let runtime = &mut *(runtime as *mut Runtime);
    match runtime
        .regexes
        .get_line_stdin(&runtime.vars.rs, &mut runtime.read_files)
    {
        Ok(res) => mem::transmute::<Str, u128>(res),
        Err(err) => fail!("unexpected error when reading line from stdin: {}", err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn next_line(runtime: *mut c_void, file: *mut c_void) -> u128 {
    let runtime = &mut *(runtime as *mut Runtime);
    let file = &*(file as *mut Str);
    match runtime
        .regexes
        .get_line(file, &runtime.vars.rs, &mut runtime.read_files)
    {
        Ok(res) => mem::transmute::<Str, u128>(res),
        Err(_) => mem::transmute::<Str, u128>("".into()),
    }
}

#[no_mangle]
pub unsafe extern "C" fn print_stdout(runtime: *mut c_void, txt: *mut c_void) {
    let newline: Str<'static> = "\n".into();
    let runtime = &mut *(runtime as *mut Runtime);
    let txt = &*(txt as *mut Str);
    if runtime.write_files.write_str_stdout(txt).is_err() {
        fail!("TODO: handle errors in file writing!")
    }
    if runtime.write_files.write_str_stdout(&newline).is_err() {
        fail!("TODO: handle errors in file writing!")
    }
}

#[no_mangle]
pub unsafe extern "C" fn print(
    runtime: *mut c_void,
    txt: *mut c_void,
    out: *mut c_void,
    append: Int,
) {
    let runtime = &mut *(runtime as *mut Runtime);
    let txt = &*(txt as *mut Str);
    let out = &*(out as *mut Str);
    if runtime
        .write_files
        .write_line(out, txt, append != 0)
        .is_err()
    {
        fail!("handle errors in file writing!")
    }
}

#[no_mangle]
pub unsafe extern "C" fn split_str(
    runtime: *mut c_void,
    to_split: *mut c_void,
    into_arr: usize,
    pat: *mut c_void,
) -> Int {
    let runtime = &mut *(runtime as *mut Runtime);
    let into_arr = mem::transmute::<usize, StrMap<Str>>(into_arr);
    let to_split = &*(to_split as *mut Str);
    let pat = &*(pat as *mut Str);
    let old_len = into_arr.len();
    if let Err(e) = runtime
        .regexes
        .split_regex_strmap(&pat, &to_split, &into_arr)
    {
        fail!("failed to split string: {}", e);
    }
    let res = (into_arr.len() - old_len) as Int;
    mem::forget((into_arr, to_split, pat));
    res
}

#[no_mangle]
pub unsafe extern "C" fn split_int(
    runtime: *mut c_void,
    to_split: *mut c_void,
    into_arr: usize,
    pat: *mut c_void,
) -> Int {
    let runtime = &mut *(runtime as *mut Runtime);
    let into_arr = mem::transmute::<usize, IntMap<Str>>(into_arr);
    let to_split = &*(to_split as *mut Str);
    let pat = &*(pat as *mut Str);
    let old_len = into_arr.len();
    if let Err(e) = runtime
        .regexes
        .split_regex_intmap(&pat, &to_split, &into_arr)
    {
        fail!("failed to split string: {}", e);
    }
    let res = (into_arr.len() - old_len) as Int;
    mem::forget((into_arr, to_split, pat));
    res
}

#[no_mangle]
pub unsafe extern "C" fn get_col(runtime: *mut c_void, col: Int) -> u128 {
    if col < 0 {
        fail!("attempt to access negative column: {}", col);
    }
    let runtime = &mut *(runtime as *mut Runtime);
    if col == 0 {
        return mem::transmute::<Str, u128>(runtime.line.clone());
    }
    if runtime.split_line.len() == 0 {
        if let Err(e) =
            runtime
                .regexes
                .split_regex(&runtime.vars.fs, &runtime.line, &mut runtime.split_line)
        {
            fail!("failed to split line: {}", e);
        }
        runtime.vars.nf = runtime.split_line.len() as Int;
    }
    let res = runtime
        .split_line
        .get(col as usize - 1)
        .unwrap_or_else(Str::default);
    mem::transmute::<Str, u128>(res)
}

#[no_mangle]
pub unsafe extern "C" fn set_col(runtime: *mut c_void, col: Int, s: *mut c_void) {
    if col < 0 {
        fail!("attempt to set negative column: {}", col);
    }
    let runtime = &mut *(runtime as *mut Runtime);
    if col == 0 {
        runtime.split_line.clear();
        ref_str(s);
        runtime.line = (*(s as *mut Str)).clone();
        runtime.vars.nf = -1;
        return;
    }
    if runtime.split_line.len() == 0 {
        if let Err(e) =
            runtime
                .regexes
                .split_regex(&runtime.vars.fs, &runtime.line, &mut runtime.split_line)
        {
            fail!("failed to split line: {}", e);
        }
        runtime.vars.nf = runtime.split_line.len() as Int;
    }
    let s = &*(s as *mut Str);
    runtime.split_line.insert(col as usize - 1, s.clone());
}

#[no_mangle]
pub unsafe extern "C" fn str_len(s: *mut c_void) -> usize {
    let s = &*(s as *mut Str);
    let res = s.len();
    mem::forget(s);
    res
}

#[no_mangle]
pub unsafe extern "C" fn concat(s1: *mut c_void, s2: *mut c_void) -> u128 {
    let s1 = &*(s1 as *mut Str);
    let s2 = &*(s2 as *mut Str);
    let res = Str::concat(s1.clone(), s2.clone());
    mem::forget((s1, s2));
    mem::transmute::<Str, u128>(res)
}

// TODO: figure out error story.

#[no_mangle]
pub unsafe extern "C" fn match_pat(runtime: *mut c_void, s: *mut c_void, pat: *mut c_void) -> Int {
    let runtime = runtime as *mut Runtime;
    let s = &*(s as *mut Str);
    let pat = &*(pat as *mut Str);
    let res = match (*runtime).regexes.match_regex(&pat, &s) {
        Ok(res) => res as Int,
        Err(e) => fail!("match_pat: {}", e),
    };
    mem::forget((s, pat));
    res
}

#[no_mangle]
pub unsafe extern "C" fn ref_str(s: *mut c_void) {
    mem::forget((&*(s as *mut Str)).clone())
}

#[no_mangle]
pub unsafe extern "C" fn drop_str(s: *mut c_void) {
    std::ptr::drop_in_place(s as *mut Str);
}

unsafe fn ref_map_generic<K, V>(m: usize) {
    mem::forget(mem::transmute::<&usize, &runtime::SharedMap<K, V>>(&m).clone())
}

unsafe fn drop_map_generic<K, V>(m: usize) {
    mem::drop(mem::transmute::<usize, runtime::SharedMap<K, V>>(m))
}

// XXX: relying on this doing the same thing regardless of type. We probably want a custom Rc to
// guarantee this.
#[no_mangle]
pub unsafe extern "C" fn ref_map(m: usize) {
    ref_map_generic::<Int, Int>(m)
}
#[no_mangle]
pub unsafe extern "C" fn drop_map(m: usize) {
    drop_map_generic::<Int, Int>(m)
}

#[no_mangle]
pub unsafe extern "C" fn int_to_str(i: Int) -> u128 {
    mem::transmute::<Str, u128>(runtime::convert::<Int, Str>(i))
}

#[no_mangle]
pub unsafe extern "C" fn float_to_str(f: Float) -> u128 {
    mem::transmute::<Str, u128>(runtime::convert::<Float, Str>(f))
}

#[no_mangle]
pub unsafe extern "C" fn str_to_int(s: *mut c_void) -> Int {
    let s = &*(s as *mut Str);
    let res = runtime::convert::<&Str, Int>(&s);
    mem::forget(s);
    res
}

#[no_mangle]
pub unsafe extern "C" fn str_to_float(s: *mut c_void) -> Float {
    let s = &*(s as *mut Str);
    let res = runtime::convert::<&Str, Float>(&s);
    mem::forget(s);
    res
}

#[no_mangle]
pub unsafe extern "C" fn load_var_str(rt: *mut c_void, var: usize) -> u128 {
    let rt = &*(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        use Variable::*;
        let res = match var {
            FS => rt.vars.fs.clone(),
            OFS => rt.vars.ofs.clone(),
            RS => rt.vars.rs.clone(),
            FILENAME => rt.vars.filename.clone(),
            ARGC | ARGV | NF | NR => fail!("non-string var={:?}", var),
        };
        mem::transmute::<Str, u128>(res)
    } else {
        fail!("invalid variable code={}", var)
    }
}

#[no_mangle]
pub unsafe extern "C" fn store_var_str(rt: *mut c_void, var: usize, s: *mut c_void) {
    let rt = &mut *(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        let s = (&*(s as *mut Str)).clone();
        use Variable::*;
        match var {
            FS => rt.vars.fs = s,
            OFS => rt.vars.ofs = s,
            RS => rt.vars.rs = s,
            FILENAME => rt.vars.filename = s,
            ARGC | ARGV | NF | NR => fail!("non-string var={:?}", var),
        };
    } else {
        fail!("invalid variable code={}", var)
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_var_int(rt: *mut c_void, var: usize) -> Int {
    let rt = &mut *(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        use Variable::*;
        match var {
            ARGC => rt.vars.argc,
            NF => {
                if rt.split_line.len() == 0 {
                    if let Err(e) =
                        rt.regexes
                            .split_regex(&rt.vars.fs, &rt.line, &mut rt.split_line)
                    {
                        fail!("failed to split line: {}", e);
                    }
                    rt.vars.nf = rt.split_line.len() as Int;
                }
                rt.vars.nf
            }
            NR => rt.vars.nr,
            OFS | FS | RS | FILENAME | ARGV => fail!("non-int variable {}", var),
        }
    } else {
        fail!("invalid variable code={}", var)
    }
}

#[no_mangle]
pub unsafe extern "C" fn store_var_int(rt: *mut c_void, var: usize, i: Int) {
    let rt = &mut *(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        use Variable::*;
        match var {
            ARGC => rt.vars.argc = i,
            NF => rt.vars.nf = i,
            NR => rt.vars.nr = i,
            OFS | FS | RS | FILENAME | ARGV => fail!("non-int variable {}", var),
        };
    } else {
        fail!("invalid variable code={}", var)
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_var_intmap(rt: *mut c_void, var: usize) -> usize {
    let rt = &*(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        use Variable::*;
        let res = match var {
            ARGV => rt.vars.argv.clone(),
            OFS | ARGC | NF | NR | FS | RS | FILENAME => fail!("non intmap-var={:?}", var),
        };
        mem::transmute::<IntMap<_>, usize>(res)
    } else {
        fail!("invalid variable code={}", var)
    }
}

#[no_mangle]
pub unsafe extern "C" fn store_var_intmap(rt: *mut c_void, var: usize, map: usize) {
    let rt = &mut *(rt as *mut Runtime);
    if let Ok(var) = Variable::try_from(var) {
        use Variable::*;
        let map = mem::transmute::<usize, IntMap<Str>>(map);
        match var {
            ARGV => rt.vars.argv = map.clone(),
            OFS | ARGC | NF | NR | FS | RS | FILENAME => fail!("non intmap-var={:?}", var),
        };
        mem::forget(map);
    } else {
        fail!("invalid variable code={}", var)
    }
}

macro_rules! str_compare_inner {
    ($name:ident, $op:tt) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(s1: *mut c_void, s2: *mut c_void) -> Int {
            let s1 = &*(s1 as *mut Str);
            let s2 = &*(s2 as *mut Str);
            let res = s1.with_str(|s1| s2.with_str(|s2| s1 $op s2)) as Int;
            mem::forget((s1, s2));
            res
        }
    }
}
macro_rules! str_compare {
    ($($name:ident ($op:tt);)*) => { $( str_compare_inner!($name, $op); )* };
}

str_compare! {
    str_lt(<); str_gt(>); str_lte(<=); str_gte(>=); str_eq(==);
}

// And now for the shenanigans for implementing map operations. There are 36 functions here; we a
// bunch of macros to handle type-specific operations. Note: we initially had a trait for these
// specific operations:
//   pub trait InTy {
//       type In;
//       type Out;
//       fn convert_in(x: &Self::In) -> &Self;
//       fn convert_out(x: Self) -> Self::Out;
//   }
// But that didn't end up working out. We had intrinsic functions with parameter types like
// <Int as InTy>::In, which had strange consequences like not being able to take the address of a
// function. We need to take the address of these functions though,  otherwise the linker on some
// platform will spuriously strip out the symbol.  Instead, we replicate this trait in the form of
// macros that match on the input type.

macro_rules! in_ty {
    (Str) => { *mut c_void };
    (Int) => { Int };
    (Float) => { Float };
}

macro_rules! out_ty {
    (Str) => {
        u128
    };
    (Int) => {
        Int
    };
    (Float) => {
        Float
    };
}

macro_rules! convert_in {
    (Str, $e:expr) => {
        &*((*$e) as *mut Str)
    };
    (Int, $e:expr) => {
        $e
    };
    (Float, $e:expr) => {
        $e
    };
}

macro_rules! convert_out {
    (Str, $e:expr) => {
        mem::transmute::<Str, u128>($e)
    };
    (Int, $e:expr) => {
        $e
    };
    (Float, $e:expr) => {
        $e
    };
}

macro_rules! map_impl_inner {
    ($alloc:ident, $lookup:ident, $len:ident, $insert:ident, $delete:ident, $contains:ident, $k:tt, $v:tt) => {
        #[no_mangle]
        pub unsafe extern "C" fn $alloc() -> usize {
            let res: runtime::SharedMap<$k, $v> = Default::default();
            mem::transmute::<runtime::SharedMap<$k, $v>, usize>(res)
        }
        #[no_mangle]
        pub unsafe extern "C" fn $len(map: usize) -> Int {
            let map = mem::transmute::<usize, runtime::SharedMap<$k, $v>>(map);
            let res = map.len();
            mem::forget(map);
            res as Int
        }
        #[no_mangle]
        pub unsafe extern "C" fn $lookup(map: usize, k: in_ty!($k)) -> out_ty!($v) {
            let map = mem::transmute::<usize, runtime::SharedMap<$k, $v>>(map);
            let key = convert_in!($k, &k);
            let res = map.get(key).unwrap_or_else(Default::default);
            mem::forget(map);
            convert_out!($v, res)
        }
        #[no_mangle]
        pub unsafe extern "C" fn $contains(map: usize, k: in_ty!($k)) -> Int {
            let map = mem::transmute::<usize, runtime::SharedMap<$k, $v>>(map);
            let key = convert_in!($k, &k);
            let res = map.get(key).is_some() as Int;
            mem::forget(map);
            res
        }
        #[no_mangle]
        pub unsafe extern "C" fn $insert(map: usize, k: in_ty!($k), v: in_ty!($v)) {
            let map = mem::transmute::<usize, runtime::SharedMap<$k, $v>>(map);
            let key = convert_in!($k, &k);
            let val = convert_in!($v, &v);
            map.insert(key.clone(), val.clone());
            mem::forget(map);
        }
        #[no_mangle]
        pub unsafe extern "C" fn $delete(map: usize, k: in_ty!($k)) {
            let map = mem::transmute::<usize, runtime::SharedMap<$k, $v>>(map);
            let key = convert_in!($k, &k);
            map.delete(key);
            mem::forget(map);
        }
    };
}

macro_rules! map_impl {
    ($($alloc:ident, $len:ident, $lookup:ident,
       $insert:ident, $delete:ident, $contains:ident, < $k:tt, $v:tt >;)*) => {
        $(
        map_impl_inner!($alloc, $lookup, $len,$insert,$delete,$contains, $k, $v);
        )*
    }
}

map_impl! {
    alloc_intint, len_intint, lookup_intint, insert_intint, delete_intint, contains_intint, <Int, Int>;
    alloc_intfloat, len_intfloat, lookup_intfloat, insert_intfloat, delete_intfloat, contains_intfloat, <Int, Float>;
    alloc_intstr, len_intstr, lookup_intstr, insert_intstr, delete_intstr, contains_intstr, <Int, Str>;
    alloc_strint, len_strint, lookup_strint, insert_strint, delete_strint, contains_strint, <Str, Int>;
    alloc_strfloat, len_strfloat, lookup_strfloat, insert_strfloat, delete_strfloat, contains_strfloat, <Str, Float>;
    alloc_strstr, len_strstr, lookup_strstr, insert_strstr, delete_strstr, contains_strstr, <Str, Str>;
}