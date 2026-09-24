use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{Connection, SqliteError};

pub struct Transaction<'a> {
    conn: &'a Connection,
    name: String,
    open: bool,
}

impl Connection {
    pub fn transaction(&self) -> Result<Transaction<'_>, SqliteError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let name = format!("sc_sp_{}", NEXT.fetch_add(1, Ordering::Relaxed));
        self.batch_execute(&format!("SAVEPOINT {name}"))?;
        Ok(Transaction {
            conn: self,
            name,
            open: true,
        })
    }
}

impl Transaction<'_> {
    pub fn commit(mut self) -> Result<(), SqliteError> {
        self.conn.batch_execute(&format!("RELEASE {}", self.name))?;
        self.open = false;
        Ok(())
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = self
                .conn
                .batch_execute(&format!("ROLLBACK TO {}", self.name));
            let _ = self.conn.batch_execute(&format!("RELEASE {}", self.name));
        }
    }
}

impl Deref for Transaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl AsRef<Connection> for Transaction<'_> {
    fn as_ref(&self) -> &Connection {
        self.conn
    }
}

#[cfg(test)]
mod tests {
    use libsqlite3_sys::{sqlite3_close, sqlite3_open};

    use crate::{Connection, SqlValue, SqliteError};

    fn open_memory() -> Connection {
        let mut db = std::ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) }, 0);
        Connection::from_raw(db)
    }

    fn close(conn: Connection) {
        unsafe { sqlite3_close(conn.as_ptr()) };
    }

    fn count(conn: &Connection) -> i64 {
        let rows = conn.query("SELECT COUNT(*) FROM t", &[]).unwrap();
        rows[0][0].as_i64().unwrap()
    }

    #[test]
    fn commit_persists_changes() {
        let conn = open_memory();
        conn.batch_execute("CREATE TABLE t(v INTEGER PRIMARY KEY)")
            .unwrap();

        let tx = conn.transaction().unwrap();
        tx.execute("INSERT INTO t(v) VALUES (?)", &[SqlValue::Integer(1)])
            .unwrap();
        tx.commit().unwrap();

        assert_eq!(count(&conn), 1);
        close(conn);
    }

    #[test]
    fn error_rolls_back_uncommitted_changes() {
        let conn = open_memory();
        conn.batch_execute("CREATE TABLE t(v INTEGER PRIMARY KEY)")
            .unwrap();

        let err = (|| -> Result<(), SqliteError> {
            let tx = conn.transaction()?;
            tx.execute("INSERT INTO t(v) VALUES (?)", &[SqlValue::Integer(1)])?;
            tx.execute("INSERT INTO t(v) VALUES (?)", &[SqlValue::Integer(1)])?;
            tx.commit()
        })();

        assert!(err.is_err());
        assert_eq!(count(&conn), 0);
        close(conn);
    }
}
