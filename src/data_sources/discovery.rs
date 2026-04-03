//! Auto-discover which AI coding tools have local data files present.

use tokscale_core::clients::ClientId;
use tokscale_core::scanner::scan_all_clients;

/// Scan the given home directory for all known AI coding tool data files.
///
/// Returns the string IDs of tools for which at least one local data file
/// was found, or (for DB-backed clients like Kilo) for which the DB path
/// exists.
pub fn discover_tools(home_dir: &str) -> Vec<String> {
    // Build the full client list (only clients that support local parsing).
    let all_clients: Vec<String> = ClientId::iter()
        .filter(|c| c.parse_local())
        .map(|c| c.as_str().to_string())
        .collect();

    let scan = scan_all_clients(home_dir, &all_clients);

    let mut found: Vec<String> = Vec::new();

    for client in ClientId::iter() {
        if !client.parse_local() {
            continue;
        }

        // For DB-backed clients the scanner puts a sentinel path in the vec
        // when the database exists; for file-backed clients it's the actual
        // matched files.
        if !scan.get(client).is_empty() {
            found.push(client.as_str().to_string());
            continue;
        }

        // Kilo and Crush store data in a single database; check the dedicated
        // ScanResult fields as a fallback.
        match client {
            ClientId::Kilo if scan.kilo_db.is_some() => {
                found.push(client.as_str().to_string());
            }
            _ => {}
        }
    }

    // Also surface Crush if its crush_dbs list is non-empty.
    if !scan.crush_dbs.is_empty()
        && !found.contains(&ClientId::Crush.as_str().to_string())
    {
        found.push(ClientId::Crush.as_str().to_string());
    }

    found
}
