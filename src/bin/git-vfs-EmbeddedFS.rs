use git2::{BranchType, Repository};
use git_vfs::embedded_vfs;
use vfs::error::VfsErrorKind;
use vfs::VfsResult;

fn main() -> VfsResult<()> {
    println!("--- Git2 with Embedded .git Repository ---");

    let temp_git_dir = embedded_vfs::extract_embedded_git_to_temp_dir()?;
    let repo_path = temp_git_dir.path();

    println!("Extracted .git to: {:?}", repo_path);

    let repo = Repository::open(repo_path)
        .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
    println!("\n--- Repository HEAD ---");
    match repo.head() {
        Ok(head) => println!("HEAD: {}", head.name().unwrap_or("detached")),
        Err(e) => println!("Error getting HEAD: {}", e),
    }

    println!("\n--- Branches ---");
    let branches = repo
        .branches(None)
        .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
    for branch in branches {
        let (branch, branch_type) = branch
            .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
        let name = branch
            .name()
            .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
        let commit = branch
            .get()
            .peel_to_commit()
            .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
        println!(
            "  {} {}: {}",
            match branch_type {
                BranchType::Local => "Local",
                BranchType::Remote => "Remote",
            },
            name.unwrap_or("unnamed branch"),
            commit.id()
        );
    }

    println!("\n--- Commit Log ---");
    let mut revwalk = repo
        .revwalk()
        .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
    revwalk
        .push_head()
        .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
    for oid in revwalk {
        let oid =
            oid.map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| vfs::VfsError::from(VfsErrorKind::Other(format!("Git error: {}", e))))?;
        println!("  Commit: {}", commit.id());
        println!(
            "  Author: {} <{}>",
            commit.author().name().unwrap_or("unknown"),
            commit.author().email().unwrap_or("unknown")
        );
        println!("  Date:   {}", commit.author().when().seconds());
        println!("  Message: {}", commit.message().unwrap_or("no message"));
        println!("----------------------------------------");
    }

    Ok(())
}
