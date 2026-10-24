execute store result score #entity_test.t_0 e2e run data get storage e2e:vars entity_test.v_iter
execute unless score #entity_test.t_0 e2e matches 1.. run return 0
execute store result score #entity_test.v_h e2e run data get storage e2e:vars entity_test.v_iter[0]
data remove storage e2e:vars entity_test.v_iter[0]
scoreboard players set #entity_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_h e2e
execute store success score #entity_test.t_0 e2e run function e2e:entity_test/m2 with storage e2e:macro
data remove storage e2e:macro h0
execute if score #entity_test.t_0 e2e matches 1 run function e2e:entity_test/b4
return run function e2e:entity_test/b3
