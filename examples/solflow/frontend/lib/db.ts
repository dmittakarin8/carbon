import Database from 'better-sqlite3';
import path from 'path';

let db: Database.Database | null = null;

function getDbPath(): string {
  const dbPath = process.env.DB_PATH || '/var/lib/solflow/solflow.db';
  
  // For development, allow relative path fallback
  return path.isAbsolute(dbPath) 
    ? dbPath 
    : path.resolve(process.cwd(), dbPath);
}

export function getDb(): Database.Database {
  if (db) {
    return db;
  }

  const resolvedPath = getDbPath();
  db = new Database(resolvedPath, { readonly: true });
  
  // Enable WAL mode for consistent reads
  db.pragma('journal_mode = WAL');
  
  return db;
}

export function getWriteDb(): Database.Database {
  const resolvedPath = getDbPath();
  const writeDb = new Database(resolvedPath, { readonly: false });
  writeDb.pragma('journal_mode = WAL');
  return writeDb;
}

export function closeDb(): void {
  if (db) {
    db.close();
    db = null;
  }
}

