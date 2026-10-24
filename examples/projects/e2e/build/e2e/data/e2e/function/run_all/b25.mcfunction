execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_zs
execute unless score #run_all.v_zk e2e < #run_all.t_0 e2e run return 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_zk e2e
function e2e:run_all/m9 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_tmp_10
scoreboard players operation #run_all.v_zt e2e += #run_all.t_0 e2e
scoreboard players add #run_all.v_zk e2e 1
return run function e2e:run_all/b25
