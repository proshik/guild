//! Bounded executable-header checks. codesign separately verifies the complete bundle.
use object::{
    LittleEndian, macho,
    read::macho::{FatArch, MachHeader, MachOFatFile32, MachOFatFile64},
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::PermissionsExt,
    path::Path,
};
const INVALID: &str = "update executable has no valid arm64 Mach-O slice";
const MAX_ARCHES: usize = 32;
pub(super) fn validate(path: &Path) -> Result<(), String> {
    let mut file = super::open_regular(path)?;
    let metadata = file.metadata().map_err(|_| INVALID)?;
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(INVALID.into());
    }
    let mut magic = [0; 8];
    file.read_exact(&mut magic).map_err(|_| INVALID)?;
    let kind = u32::from_be_bytes(magic[..4].try_into().unwrap());
    if kind != macho::FAT_MAGIC && kind != macho::FAT_MAGIC_64 {
        return thin(&mut file, 0, metadata.len(), None);
    }
    let count = u32::from_be_bytes(magic[4..].try_into().unwrap()) as usize;
    if count == 0 || count > MAX_ARCHES {
        return Err(INVALID.into());
    }
    let width = if kind == macho::FAT_MAGIC { 20 } else { 32 };
    let table_len = 8 + count * width;
    if table_len as u64 > metadata.len() {
        return Err(INVALID.into());
    }
    let mut table = vec![0; table_len];
    table[..8].copy_from_slice(&magic);
    file.read_exact(&mut table[8..]).map_err(|_| INVALID)?;
    if kind == macho::FAT_MAGIC {
        let fat = MachOFatFile32::parse(table.as_slice()).map_err(|_| INVALID)?;
        universal(&mut file, metadata.len(), table_len as u64, fat.arches())
    } else {
        let fat = MachOFatFile64::parse(table.as_slice()).map_err(|_| INVALID)?;
        universal(&mut file, metadata.len(), table_len as u64, fat.arches())
    }
}
fn universal<A: FatArch>(
    file: &mut File,
    len: u64,
    table_end: u64,
    arches: &[A],
) -> Result<(), String> {
    let mut ranges = Vec::with_capacity(arches.len());
    let mut selected = None;
    for arch in arches {
        let (offset, size) = arch.file_range();
        let end = offset.checked_add(size).ok_or(INVALID)?;
        let alignment = 1u64.checked_shl(arch.align()).ok_or(INVALID)?;
        if offset < table_end || size < 32 || end > len || offset % alignment != 0 {
            return Err(INVALID.into());
        }
        ranges.push((offset, end));
        if arch.cputype() == macho::CPU_TYPE_ARM64 {
            if selected.is_some() || !supported_subtype(arch.cpusubtype()) {
                return Err(INVALID.into());
            }
            selected = Some((offset, size, arch.cpusubtype()));
        }
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(INVALID.into());
    }
    let (offset, size, subtype) = selected.ok_or(INVALID)?;
    thin(file, offset, size, Some(subtype))
}
fn supported_subtype(value: u32) -> bool {
    // ARM64_ALL and ARM64_V8 run on the supported arm64 Macs; arm64e is distinct.
    matches!(value & !macho::CPU_SUBTYPE_MASK, 0 | 1)
}
fn thin(
    file: &mut File,
    offset: u64,
    size: u64,
    expected_subtype: Option<u32>,
) -> Result<(), String> {
    if size < 32 {
        return Err(INVALID.into());
    }
    let mut bytes = [0; 32];
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|_| INVALID)?;
    let header =
        macho::MachHeader64::<LittleEndian>::parse(bytes.as_slice(), 0).map_err(|_| INVALID)?;
    let commands = header.sizeofcmds(LittleEndian) as u64;
    let subtype = header.cpusubtype(LittleEndian);
    if !header.is_little_endian()
        || header.cputype(LittleEndian) != macho::CPU_TYPE_ARM64
        || !supported_subtype(subtype)
        || expected_subtype.is_some_and(|expected| expected != subtype)
        || header.filetype(LittleEndian) != macho::MH_EXECUTE
        || commands > size - 32
        || u64::from(header.ncmds(LittleEndian)) * 8 > commands
    {
        return Err(INVALID.into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn thin_bytes(cpu: u32, subtype: u32, kind: u32) -> Vec<u8> {
        [macho::MH_MAGIC_64, cpu, subtype, kind, 1, 8, 0, 0, 0, 8]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect()
    }
    fn check(bytes: &[u8]) -> bool {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("executable");
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        validate(&path).is_ok()
    }
    fn fat(wide: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        let magic = if wide {
            macho::FAT_MAGIC_64
        } else {
            macho::FAT_MAGIC
        };
        for word in [magic, 2, macho::CPU_TYPE_X86_64, 3] {
            bytes.extend(word.to_be_bytes());
        }
        if wide {
            bytes.extend(128u64.to_be_bytes());
            bytes.extend(40u64.to_be_bytes());
        } else {
            bytes.extend(128u32.to_be_bytes());
            bytes.extend(40u32.to_be_bytes());
        }
        bytes.extend(0u32.to_be_bytes());
        if wide {
            bytes.extend(0u32.to_be_bytes());
        }
        bytes.extend(macho::CPU_TYPE_ARM64.to_be_bytes());
        bytes.extend(0u32.to_be_bytes());
        if wide {
            bytes.extend(256u64.to_be_bytes());
            bytes.extend(40u64.to_be_bytes());
        } else {
            bytes.extend(256u32.to_be_bytes());
            bytes.extend(40u32.to_be_bytes());
        }
        bytes.extend(0u32.to_be_bytes());
        if wide {
            bytes.extend(0u32.to_be_bytes());
        }
        bytes.resize(128, 0);
        bytes.extend(thin_bytes(macho::CPU_TYPE_X86_64, 3, macho::MH_EXECUTE));
        bytes.resize(256, 0);
        bytes.extend(thin_bytes(macho::CPU_TYPE_ARM64, 0, macho::MH_EXECUTE));
        bytes
    }
    #[test]
    fn executable_architecture_checks_reject_wrong_incomplete_and_non_executable_slices() {
        assert!(check(&thin_bytes(
            macho::CPU_TYPE_ARM64,
            0,
            macho::MH_EXECUTE
        )));
        assert!(check(&thin_bytes(
            macho::CPU_TYPE_ARM64,
            1,
            macho::MH_EXECUTE
        )));
        for (cpu, subtype, kind) in [
            (macho::CPU_TYPE_X86_64, 3, macho::MH_EXECUTE),
            (macho::CPU_TYPE_ARM64, 2, macho::MH_EXECUTE),
            (macho::CPU_TYPE_ARM64, 0, macho::MH_DYLIB),
        ] {
            assert!(!check(&thin_bytes(cpu, subtype, kind)));
        }
        let valid = thin_bytes(macho::CPU_TYPE_ARM64, 0, macho::MH_EXECUTE);
        assert!(!check(&valid[..31]));
        assert!(!check(&valid[..39]), "truncated load command region");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("not-executable");
        fs::write(&path, &valid).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(validate(&path).is_err());
    }
    #[test]
    fn universal_slices_are_bounded_nonoverlapping_and_match_their_table() {
        for wide in [false, true] {
            let valid = fat(wide);
            assert!(check(&valid));
            let mut wrong_slice = valid.clone();
            wrong_slice[260..264].copy_from_slice(&macho::CPU_TYPE_X86_64.to_le_bytes());
            assert!(!check(&wrong_slice));
            let mut count = valid.clone();
            count[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(!check(&count));
            assert!(!check(&valid[..valid.len() - 1]));
        }
        let mut overflow = fat(true);
        overflow[48..56].copy_from_slice(&u64::MAX.to_be_bytes());
        assert!(!check(&overflow));
        let mut overlapping = fat(false);
        overlapping[36..40].copy_from_slice(&128u32.to_be_bytes());
        assert!(!check(&overlapping));
    }
}
