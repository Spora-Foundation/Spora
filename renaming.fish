# 替换 tondi → tondi
for file in (fd . --type f --exclude .git --exclude target --exclude .idea | rg -l '\btondi\b')
    sed -i 's/\btondi\b/tondi/g' $file
end

# 替换 tondi → Tondi
for file in (fd . --type f --exclude .git --exclude target --exclude .idea | rg -l '\btondi\b')
    sed -i 's/\btondi\b/Tondi/g' $file
end

# 替换 Tondid → Tondid
for file in (fd . --type f --exclude .git --exclude target --exclude .idea | rg -l '\btondid\b')
    sed -i 's/\btondid\b/Tondid/g' $file
end

# 替换 TND → TND
for file in (fd . --type f --exclude .git --exclude target --exclude .idea | rg -l '\bKAS\b')
    sed -i 's/\bKAS\b/TND/g' $file
end
