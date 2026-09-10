#define SQLITE_CORE 1
#include <sqlite3ext.h>

int sqlite_clientdata_set(
    const sqlite3_api_routines *api,
    sqlite3 *db,
    const char *name,
    void *data,
    void (*destructor)(void *)
) {
    if (api == 0 || api->libversion_number() < 3044000 || api->set_clientdata == 0)  {
        return SQLITE_MISUSE;
    }

    return api->set_clientdata(db, name, data, destructor);
}

void *sqlite_clientdata_get(
    const sqlite3_api_routines *api,
    sqlite3 *db,
    const char *name
) {
    if (api == 0 || api->libversion_number() < 3044000 || api->set_clientdata == 0) {
        return 0;
    }

    return api->get_clientdata(db, name);
}
