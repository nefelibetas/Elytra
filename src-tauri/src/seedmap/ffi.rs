use std::ffi::{c_char, c_int, c_void, CStr, CString};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawStructure {
    pub kind: i32,
    pub x: i32,
    pub z: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawChunk {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Copy)]
pub struct StructureSearch {
    pub mc: i32,
    pub seed: i64,
    pub kind: i32,
    pub center_x: i32,
    pub center_z: i32,
    pub radius: i32,
    pub cap: i32,
}

extern "C" {
    fn elytra_gen_new(mc_name: *const c_char, seed: u64) -> *mut c_void;
    fn elytra_gen_free(gen: *mut c_void);
    fn elytra_mc_from_name(name: *const c_char) -> c_int;
    fn elytra_biome_at(gen: *mut c_void, scale: c_int, x: c_int, y: c_int, z: c_int) -> c_int;
    fn elytra_gen_biomes(
        gen: *mut c_void,
        scale: c_int,
        x: c_int,
        z: c_int,
        sx: c_int,
        sz: c_int,
        y: c_int,
        out: *mut c_int,
    ) -> c_int;
    fn elytra_biome_name(mc: c_int, biome: c_int) -> *const c_char;
    fn elytra_max_y_quart(mc: c_int) -> c_int;
    fn elytra_collect_structures(
        gen: *mut c_void,
        mc: c_int,
        seed: u64,
        rx: c_int,
        rz: c_int,
        out: *mut RawStructure,
        cap: c_int,
    ) -> c_int;
    fn elytra_find_structures(
        gen: *mut c_void,
        mc: c_int,
        seed: u64,
        kind: c_int,
        center_x: c_int,
        center_z: c_int,
        radius: c_int,
        out: *mut RawStructure,
        cap: c_int,
    ) -> c_int;
    fn elytra_collect_slime_chunks(
        seed: u64,
        rx: c_int,
        rz: c_int,
        out: *mut RawChunk,
        cap: c_int,
    ) -> c_int;
    fn elytra_find_slime_chunks(
        seed: u64,
        center_x: c_int,
        center_z: c_int,
        radius: c_int,
        out: *mut RawChunk,
        cap: c_int,
    ) -> c_int;
}

pub struct Generator(*mut c_void);

#[derive(Clone, Copy)]
pub struct BiomeRange {
    pub scale: i32,
    pub x: i32,
    pub z: i32,
    pub sx: i32,
    pub sz: i32,
    pub y: i32,
}

impl Generator {
    pub fn new(version: &str, seed: i64) -> Result<Self, String> {
        let name = CString::new(version).map_err(|_| "版本名无效\n包含 NUL 字符".to_string())?;
        let ptr = unsafe { elytra_gen_new(name.as_ptr(), seed as u64) };
        if ptr.is_null() {
            return Err(format!("版本不受支持\n{version}"));
        }
        Ok(Self(ptr))
    }

    pub fn biome_at(&self, scale: i32, x: i32, y: i32, z: i32) -> i32 {
        unsafe { elytra_biome_at(self.0, scale, x, y, z) }
    }

    pub fn gen_biomes(&self, range: BiomeRange, out: &mut [i32]) -> Result<(), String> {
        if out.len() < (range.sx as usize) * (range.sz as usize) {
            return Err("瓦片缓冲尺寸不足\n内部参数错误".to_string());
        }
        let rc = unsafe {
            elytra_gen_biomes(
                self.0,
                range.scale,
                range.x,
                range.z,
                range.sx,
                range.sz,
                range.y,
                out.as_mut_ptr(),
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("群系生成失败\n错误码 {rc}"))
        }
    }

    pub fn structures(&self, mc: i32, seed: i64, rx: i32, rz: i32) -> Vec<RawStructure> {
        let cap = 512usize;
        let mut out = vec![
            RawStructure {
                kind: 0,
                x: 0,
                z: 0
            };
            cap
        ];
        let n = unsafe {
            elytra_collect_structures(
                self.0,
                mc,
                seed as u64,
                rx,
                rz,
                out.as_mut_ptr(),
                cap as c_int,
            )
        };
        let n = n.max(0) as usize;
        out.truncate(n.min(cap));
        out
    }

    pub fn find_structures(&self, q: StructureSearch) -> Vec<RawStructure> {
        let cap = q.cap.clamp(1, 64) as usize;
        let mut out = vec![
            RawStructure {
                kind: 0,
                x: 0,
                z: 0
            };
            cap
        ];
        let n = unsafe {
            elytra_find_structures(
                self.0,
                q.mc,
                q.seed as u64,
                q.kind,
                q.center_x,
                q.center_z,
                q.radius,
                out.as_mut_ptr(),
                cap as c_int,
            )
        };
        let n = n.max(0) as usize;
        out.truncate(n.min(cap));
        out
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        unsafe { elytra_gen_free(self.0) };
    }
}

pub fn mc_from_name(version: &str) -> Option<i32> {
    let name = CString::new(version).ok()?;
    let mc = unsafe { elytra_mc_from_name(name.as_ptr()) };
    (mc >= 0).then_some(mc)
}

pub fn biome_name(mc: i32, biome: i32) -> String {
    unsafe {
        let ptr = elytra_biome_name(mc, biome);
        if ptr.is_null() {
            return format!("unknown:{biome}");
        }
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

pub fn max_y_quart(mc: i32) -> i32 {
    unsafe { elytra_max_y_quart(mc) }
}

pub fn collect_slime_chunks(seed: i64, rx: i32, rz: i32) -> Vec<RawChunk> {
    let cap = 1024usize;
    let mut out = vec![RawChunk { x: 0, z: 0 }; cap];
    let n =
        unsafe { elytra_collect_slime_chunks(seed as u64, rx, rz, out.as_mut_ptr(), cap as c_int) };
    let n = n.max(0) as usize;
    out.truncate(n.min(cap));
    out
}

pub fn find_slime_chunks(
    seed: i64,
    center_x: i32,
    center_z: i32,
    radius: i32,
    cap: i32,
) -> Vec<RawChunk> {
    let cap = cap.clamp(1, 64) as usize;
    let mut out = vec![RawChunk { x: 0, z: 0 }; cap];
    let n = unsafe {
        elytra_find_slime_chunks(
            seed as u64,
            center_x,
            center_z,
            radius,
            out.as_mut_ptr(),
            cap as c_int,
        )
    };
    let n = n.max(0) as usize;
    out.truncate(n.min(cap));
    out
}
