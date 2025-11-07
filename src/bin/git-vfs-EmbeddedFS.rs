use vfs::VfsResult;
use git_vfs::embedded_vfs;
use git2::{Repository, BranchType, ObjectType};

fn main() -> VfsResult<()> {
    println!("--- Git2 with Embedded .git Repository ---");

    let temp_git_dir = embedded_vfs::extract_embedded_git_to_temp_dir()?;
    let repo_path = temp_git_dir.path();

    println!("Extracted .git to: {:?}", repo_path);

    let repo = Repository::open(repo_path).map_err(|e| vfs::VfsError::Other(e.into()))?;

    println!("\n--- Repository HEAD ---");
    match repo.head() {
        Ok(head) => println!("HEAD: {}", head.name().unwrap_or("detached")),
        Err(e) => println!("Error getting HEAD: {}", e),
    }

    println!("\n--- Branches ---");
    let branches = repo.branches(None).map_err(|e| vfs::VfsError::Other(e.into()))?;
    for branch in branches {
        let (branch, branch_type) = branch.map_err(|e| vfs::VfsError::Other(e.into()))?;
        let name = branch.name().map_err(|e| vfs::VfsError::Other(e.into()))?.unwrap_or("invalid");
        let commit = branch.get().peel_to_commit().map_err(|e| vfs::VfsError::Other(e.into()))?;
        println!("  {} {}: {}", match branch_type {
            BranchType::Local => "Local",
            BranchType::Remote => "Remote",
        }, name, commit.id());
    }

    println!("\n--- Commit Log ---");
    let mut revwalk = repo.revwalk().map_err(|e| vfs::VfsError::Other(e.into()))?;
    revwalk.push_head().map_err(|e| vfs::VfsError::Other(e.into()))?;
    for oid in revwalk {
        let oid = oid.map_err(|e| vfs::VfsError::Other(e.into()))?;
        let commit = repo.find_commit(oid).map_err(|e| vfs::VfsError::Other(e.into()))?;
        println!("  Commit: {}", commit.id());
        println!("  Author: {} <{}>", commit.author().name().unwrap_or("unknown"), commit.author().email().unwrap_or("unknown"));
        println!("  Date:   {}", commit.author().when().seconds());
        println!("  Message: {}", commit.message().unwrap_or("no message"));
        println!("----------------------------------------");
    }

    Ok(())
}
