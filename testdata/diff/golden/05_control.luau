-- Compiled with sloptor v2.3.2
local limit = 10
local total = 0
for i = 0, limit - 1 do
	total += i
end
for i = 10, 1, -2 do
	total += 1
	if i == 4 then
		continue
	end
	if i == 2 then
		break
	end
end
local count = 0
while count < 3 do
	count += 1
end
repeat
	do
		count -= 1
	end
until not (count > 0)
if total > 40 then
	print("big", total)
elseif total > 20 then
	print("medium")
else
	print("small")
end
do
	local scoped = 1
	print(scoped)
end
print(total, count)
return nil
