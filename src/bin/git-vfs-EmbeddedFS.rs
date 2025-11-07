use vfs::VfsResult;
use git_vfs::embedded_vfs;

fn main() -> VfsResult<()> {
    embedded_vfs::create_and_test_embedded_fs()
}