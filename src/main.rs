use git_vfs::*;
fn main() {
    let mut git_vfs = GitVfs::new();

    let blob_data = b"Hello, git virtual world!";

    let blob_sha256 = git_vfs.data_sha256(blob_data);
    let blob_hash = git_vfs
        .create_blob(blob_data)
        .expect("Failed to create blob");

    let blob_content = git_vfs.get_object(&blob_hash).expect("Failed to get blob");
    println!("blob_content: {}", String::from_utf8_lossy(&blob_content));
    println!("blob_sha256: {}", blob_sha256);

    git_vfs
        .create_ref("refs/heads/main", &blob_hash)
        .expect("Failed to create ref");
    git_vfs
        .set_head("refs/heads/main")
        .expect("failed to set head");

    let head_ref = git_vfs.get_head().expect("failed to get head");
    println!("HEAD: {head_ref}");

    let main_ref_hash = git_vfs
        .get_ref("refs/heads/main")
        .expect("failed to get ref");
    println!("Main ref hash: {main_ref_hash}");

    git_vfs
        .update_ref("refs/heads/main", "new_hash")
        .expect("failed to update ref");

    let main_ref_hash = git_vfs
        .get_ref("refs/heads/main")
        .expect("failed to get ref");
    println!("Updated Main ref hash: {main_ref_hash}");
}
