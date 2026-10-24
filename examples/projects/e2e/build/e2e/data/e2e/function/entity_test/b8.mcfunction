scoreboard players add #entity_test.v_n_2 e2e 1
scoreboard players set #new_id e2e 0
execute as @s run function e2e:_ensure_id
scoreboard players operation #entity_test.v_hv_3 e2e = #new_id e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_hv_3 e2e
function e2e:entity_test/m11 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #new_id e2e 0
execute as @s run function e2e:_ensure_id
scoreboard players operation #entity_test.v_hv_4 e2e = #new_id e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_hv_4 e2e
function e2e:entity_test/m12 with storage e2e:macro
data remove storage e2e:macro h0
