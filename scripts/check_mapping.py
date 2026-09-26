import sqlite3, os
db = os.path.join(os.environ['APPDATA'], 'com.tokencounter.gateway', 'app.db')
conn = sqlite3.connect(db)
for row in conn.execute("SELECT display_name, actual_model FROM provider_models WHERE display_name='claude-opus-5'"):
    print(row)
conn.close()
