execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_iter
execute unless score #run_all.t_0 e2e matches 1.. run return 0
execute store result score #run_all.v_x e2e run data get storage e2e:vars run_all.v_iter[0]
data remove storage e2e:vars run_all.v_iter[0]
scoreboard players operation #run_all.v_ls e2e += #run_all.v_x e2e
return run function e2e:run_all/b13
