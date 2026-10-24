execute unless score #run_all.v_brk e2e matches 0 run return 0
scoreboard players set #run_all.v_cont e2e 0
scoreboard players add #run_all.v_i e2e 1
execute if score #run_all.v_i e2e matches 21.. run function e2e:run_all/b1
execute if score #run_all.v_brk e2e matches 0 run function e2e:run_all/b2
return run function e2e:run_all/b0
